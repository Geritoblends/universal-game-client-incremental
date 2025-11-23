use anyhow::{anyhow, Result};
use buddy_alloc::buddy_alloc::{BuddyAlloc, BuddyAllocParam};
use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use wasmtime::*;

// --- MEMORY LAYOUT ---
const HEAP_START_OFFSET: usize = 15 * 1024 * 1024; // 15MB
const HEAP_SIZE: usize = 1 * 1024 * 1024;          // 1MB

pub struct SystemAllocator(BuddyAlloc);
unsafe impl Send for SystemAllocator {}

pub struct HostState {
    pub instances: HashMap<String, Instance>,
    pub shared_memory: SharedMemory,
    // [REMOVED] pub table: Option<Table>, <-- No more global table
    pub next_memory_offset: i32,
    pub next_stack_offset: i32,
    // [REMOVED] pub next_table_offset: i32, <-- No more table tetris
    pub heap_allocator: Arc<Mutex<SystemAllocator>>,
}

unsafe fn shared_memory_slice(data: &[UnsafeCell<u8>]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len()) }
}

// --- EXPORTS ---

pub fn host_alloc(mut caller: Caller<'_, HostState>, size: i32) -> i32 {
    let memory = caller.data().shared_memory.clone();
    let mem_base = memory.data().as_ptr() as usize;

    let mut wrapper = caller.data().heap_allocator.lock().unwrap();
    let ptr = wrapper.0.malloc(size as usize);

    if ptr.is_null() { return 0; }

    let offset = (ptr as usize) - mem_base;
    if offset > 16777216 {
        eprintln!("CRITICAL: Allocator returned out-of-bounds offset: {}", offset);
    }
    offset as i32
}

pub fn host_dealloc(mut caller: Caller<'_, HostState>, ptr: i32, _size: i32) {
    if ptr == 0 { return; }
    let memory = caller.data().shared_memory.clone();
    let mem_base = memory.data().as_ptr() as usize;
    let host_ptr = (mem_base + ptr as usize) as *mut u8;

    let mut wrapper = caller.data().heap_allocator.lock().unwrap();
    wrapper.0.free(host_ptr);
}

pub fn host_print(caller: Caller<'_, HostState>, message_ptr: i32, message_len: i32) -> Result<()> {
    let mem_data = caller.data().shared_memory.data();
    let mem_slice = unsafe { shared_memory_slice(mem_data) };
    
    let message_bytes = &mem_slice[message_ptr as usize..(message_ptr + message_len) as usize];
    let message = String::from_utf8_lossy(message_bytes);
    println!("[Guest Log] {}", message);
    Ok(())
}

pub fn call(
    mut caller: Caller<'_, HostState>,
    instance_id_ptr: i32, instance_id_len: i32,
    func_name_ptr: i32, func_name_len: i32,
    payload_ptr: i32, payload_len: i32,
) -> Result<i64> {
    let (mem_data, instances) = {
        let data = caller.data();
        (data.shared_memory.data(), &data.instances)
    };
    let mem_slice = unsafe { shared_memory_slice(mem_data) };

    let instance_id = String::from_utf8_lossy(
        &mem_slice[instance_id_ptr as usize..(instance_id_ptr + instance_id_len) as usize]
    ).to_string();
    
    let func_name = String::from_utf8_lossy(
        &mem_slice[func_name_ptr as usize..(func_name_ptr + func_name_len) as usize]
    ).to_string();

    let instance = instances.get(&instance_id)
        .ok_or_else(|| anyhow!("instance '{}' not found", instance_id))?
        .clone();

    let func = instance.get_export(&mut caller, &func_name)
        .and_then(|e| e.into_func())
        .ok_or_else(|| anyhow!("function '{}' not found in '{}'", func_name, instance_id))?;

    let typed = func.typed::<(i32, i32), i64>(&caller)?;
    let result = typed.call(&mut caller, (payload_ptr, payload_len))?;

    Ok(result)
}

// --- DYNAMIC LINKER ---

pub fn instantiate_plugin(
    base_linker: &Linker<HostState>,
    store: &mut Store<HostState>,
    module: &Module,
    name: &str, 
) -> Result<Instance> {
    
    println!("--- Instantiating {} ---", name);

    // 1. Assign Slots (Memory & Stack still shared/tiled)
    let memory_base = store.data().next_memory_offset;
    store.data_mut().next_memory_offset += 1024 * 1024; 

    let stack_base = store.data().next_stack_offset;
    store.data_mut().next_stack_offset += 64 * 1024; 

    // [FIX] PRIVATE TABLES
    // We do NOT increment a global table offset. 
    // Every plugin gets its own table starting at 0.
    let table_base = 0;

    // 2. Create the PRIVATE Table for this plugin
    let local_table = Table::new(
        &mut *store,
        TableType::new(RefType::FUNCREF, 1000, None),
        Ref::Func(None),
    )?;

    // 3. Create Globals
    let memory_base_global = Global::new(
        &mut *store,
        GlobalType::new(ValType::I32, Mutability::Const),
        Val::I32(memory_base),
    )?;

    // Table Base is ALWAYS 0 now
    let table_base_global = Global::new(
        &mut *store,
        GlobalType::new(ValType::I32, Mutability::Const),
        Val::I32(table_base),
    )?;

    let stack_pointer_global = Global::new(
        &mut *store,
        GlobalType::new(ValType::I32, Mutability::Var),
        Val::I32(stack_base),
    )?;

    // 4. Clone Linker
    let mut instance_linker = base_linker.clone();

    // 5. Define Imports
    instance_linker.define(&store, "env", "__memory_base", memory_base_global)?;
    instance_linker.define(&store, "env", "__table_base", table_base_global)?;
    instance_linker.define(&store, "env", "__stack_pointer", stack_pointer_global)?;
    
    // [FIX] Define the PRIVATE table as the import
    instance_linker.define(&store, "env", "__indirect_function_table", local_table)?;

    // 6. Instantiate
    let instance = instance_linker.instantiate(&mut *store, module)?;

    // 7. Initialize
    if let Some(func) = instance.get_func(&mut *store, "__wasm_call_ctors") {
        let typed = func.typed::<(), ()>(&mut *store)?;
        typed.call(&mut *store, ())?;
    }

    Ok(instance)
}

pub fn setup_runtime() -> Result<(Store<HostState>, Instance, Instance)> {
    let mut config = Config::new();
    config.wasm_threads(true); 

    let engine = Engine::new(&config)?;

    let memory_type = MemoryType::shared(256, 256);
    let memory = SharedMemory::new(&engine, memory_type)?;

    let heap_ptr = unsafe { memory.data().as_ptr().add(HEAP_START_OFFSET) };
    let param = BuddyAllocParam::new(heap_ptr as *const u8, HEAP_SIZE, 16);
    let allocator = unsafe { BuddyAlloc::new(param) };

    let host_state = HostState {
        instances: HashMap::new(),
        shared_memory: memory.clone(),
        // table: None, // Removed
        next_memory_offset: 1024 * 1024,
        next_stack_offset: 65536,
        // next_table_offset: 0, // Removed
        heap_allocator: Arc::new(Mutex::new(SystemAllocator(allocator))),
    };

    let mut store = Store::new(&engine, host_state);
    
    // --- BASE LINKER ---
    let mut linker = Linker::new(&engine);
    linker.allow_shadowing(true);
    
    // Note: We do NOT define __indirect_function_table here anymore.
    // It is defined inside instantiate_plugin per instance.

    linker.define(&store, "env", "memory", memory.clone())?;
    linker.func_wrap("env", "call", call)?;
    linker.func_wrap("env", "host_print", host_print)?;
    linker.func_wrap("env", "host_alloc", host_alloc)?;
    linker.func_wrap("env", "host_dealloc", host_dealloc)?;

    let module_core = Module::from_file(&engine, "plugins/tasksapp-core/target/wasm32-unknown-unknown/release/tasksapp_core.wasm")?;
    let module_client = Module::from_file(&engine, "plugins/tasksapp-client/target/wasm32-unknown-unknown/release/tasksapp_client.wasm")?;

    let instance_core = instantiate_plugin(&linker, &mut store, &module_core, "Core")?;
    let instance_client = instantiate_plugin(&linker, &mut store, &module_client, "Client")?;

    store.data_mut().instances.insert("tasksapp_core".to_string(), instance_core.clone());
    store.data_mut().instances.insert("tasksapp_client".to_string(), instance_client.clone());

    Ok((store, instance_core, instance_client))
}
