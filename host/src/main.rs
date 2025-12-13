use anyhow::{anyhow, Result};
use buddy_alloc::buddy_alloc::{BuddyAlloc, BuddyAllocParam};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use wasmtime::*;

// --- CONFIG ---
const HEAP_START_OFFSET: usize = 15 * 1024 * 1024; // Shared heap starts at 15MB
const HEAP_SIZE: usize = 1 * 1024 * 1024; // 1MB Shared Heap

// Thread-safe allocator wrapper
pub struct SystemAllocator(BuddyAlloc);
unsafe impl Send for SystemAllocator {}

pub struct HostState {
    pub instances: HashMap<String, Instance>,
    pub shared_memory: SharedMemory,
    pub next_memory_offset: i32,
    pub next_stack_offset: i32,
    pub heap_allocator: Arc<Mutex<SystemAllocator>>,
}

// --- STANDARD HOST FUNCTIONS ---

pub fn host_alloc(caller: Caller<'_, HostState>, size: i32) -> i32 {
    let memory = caller.data().shared_memory.clone();
    let mem_base = memory.data().as_ptr() as usize;
    let mut wrapper = caller.data().heap_allocator.lock().unwrap();
    let ptr = wrapper.0.malloc(size as usize);
    if ptr.is_null() {
        return 0;
    }
    let offset = (ptr as usize) - mem_base;
    if offset > 16 * 1024 * 1024 {
        return 0;
    }
    offset as i32
}

pub fn host_dealloc(caller: Caller<'_, HostState>, ptr: i32, _size: i32) {
    if ptr == 0 {
        return;
    }
    let memory = caller.data().shared_memory.clone();
    let mem_base = memory.data().as_ptr() as usize;
    let host_ptr = (mem_base + ptr as usize) as *mut u8;
    let mut wrapper = caller.data().heap_allocator.lock().unwrap();
    wrapper.0.free(host_ptr);
}

pub fn host_print(caller: Caller<'_, HostState>, ptr: i32, len: i32) -> Result<()> {
    let mem = caller.data().shared_memory.data();
    if ptr < 0 || len < 0 || (ptr as usize + len as usize) > mem.len() {
        return Ok(());
    }

    // FIX: Cast UnsafeCell pointer to const u8 pointer
    let slice = unsafe {
        std::slice::from_raw_parts(mem.as_ptr().add(ptr as usize) as *const u8, len as usize)
    };
    println!("[WASM] {}", String::from_utf8_lossy(slice));
    Ok(())
}

// --- THE GENERIC RUNTIME LINKER ---

pub fn host_link_call(
    mut caller: Caller<'_, HostState>,
    target_mod_ptr: i32,
    target_mod_len: i32,
    target_func_ptr: i32,
    target_func_len: i32,
    local_fn_idx: i32,
    payload_ptr: i32,
    payload_len: i32,
) -> Result<()> {
    let mem = caller.data().shared_memory.data();

    // 1. Read Target Module Name
    let target_mod_name = unsafe {
        let ptr = mem.as_ptr().add(target_mod_ptr as usize) as *const u8; // FIX: Cast
        String::from_utf8_lossy(std::slice::from_raw_parts(ptr, target_mod_len as usize))
            .to_string()
    };

    // 2. Read Target Hook Name
    let target_func_name = unsafe {
        let ptr = mem.as_ptr().add(target_func_ptr as usize) as *const u8; // FIX: Cast
        String::from_utf8_lossy(std::slice::from_raw_parts(ptr, target_func_len as usize))
            .to_string()
    };

    // 3. Find the Target Instance
    let target_instance = caller
        .data()
        .instances
        .get(&target_mod_name)
        .cloned()
        .ok_or(anyhow!(
            "Target module '{}' not found. Is it loaded?",
            target_mod_name
        ))?;

    // 4. Get the Caller's Function Pointer
    let extern_val = caller
        .get_export("__indirect_function_table")
        .or_else(|| caller.get_export("table"))
        .ok_or(anyhow!(
            "Caller plugin missing table export. (Did you add -C link-arg=--export-table?)"
        ))?;
    let caller_table = extern_val
        .into_table()
        .ok_or(anyhow!("Caller export 'table' is not a table"))?;

    // FIX: Use `unwrap_func()` instead of `unwrap_funcref()`
    // `unwrap_func()` returns `Option<Func>`, so we unwrap that Option too.
    let func_ref = caller_table
        .get(&mut caller, local_fn_idx as u32)
        .ok_or(anyhow!("Invalid function index {}", local_fn_idx))?
        .unwrap_func()
        .ok_or(anyhow!(
            "Table element at {} is null/not a func",
            local_fn_idx
        ))?
        .clone();

    // 5. Inject the Function into the Target's Table
    let target_table = target_instance
        .get_table(&mut caller, "__indirect_function_table")
        .ok_or(anyhow!(
            "Target module '{}' missing '__indirect_function_table'",
            target_mod_name
        ))?;

    // Grow the target table by 1 to make room
    let new_idx = target_table.size(&caller);
    target_table.grow(&mut caller, 1, Ref::Func(Some(func_ref)))?;

    // 6. Call the Target's Hook to notify them
    let hook = target_instance
        .get_typed_func::<(i32, i32, i32), ()>(&mut caller, &target_func_name)
        .map_err(|_| {
            anyhow!(
                "Target hook '{}' not found or wrong signature",
                target_func_name
            )
        })?;

    hook.call(&mut caller, (new_idx as i32, payload_ptr, payload_len))?;

    Ok(())
}

// --- MODULE INSTANTIATOR (PRIVATE TABLES) ---
// host/src/main.rs

pub fn instantiate_plugin(
    base_linker: &Linker<HostState>,
    store: &mut Store<HostState>,
    module: &Module,
    name: &str,
) -> Result<Instance> {
    println!("📦 [HOST] Linking Plugin: {} ...", name);

    // 1. Setup Memory & Stack Offsets (Same as before)
    let mem_offset = store.data().next_memory_offset;
    store.data_mut().next_memory_offset += 1024 * 1024;

    let stack_offset = store.data().next_stack_offset;
    store.data_mut().next_stack_offset += 64 * 1024;

    let table_base = 0;

    let globals = [
        ("__memory_base", mem_offset),
        ("__stack_pointer", stack_offset),
        ("__table_base", table_base),
    ];

    let mut instance_linker = base_linker.clone();
    for (name, val) in globals {
        let mutability = if name == "__stack_pointer" {
            Mutability::Var
        } else {
            Mutability::Const
        };
        let g = Global::new(
            &mut *store,
            GlobalType::new(ValType::I32, mutability),
            Val::I32(val),
        )?;
        instance_linker.define(&store, "env", name, g)?;
    }

    // 2. DYNAMIC TABLE SIZING (The Fix)
    // We scan the module's imports to see exactly what size table it wants.
    let mut table_min = 256; // Safe default
    let mut table_max = None;

    for import in module.imports() {
        if import.module() == "env" && import.name() == "__indirect_function_table" {
            if let ExternType::Table(ty) = import.ty() {
                table_min = ty.minimum();
                table_max = ty.maximum();
                println!("   Detected Table Requirement: min={}", table_min);
            }
        }
    }

    // Create the table with the detected size
    let local_table = Table::new(
        &mut *store,
        TableType::new(RefType::FUNCREF, table_min, table_max),
        Ref::Func(None),
    )?;

    instance_linker.define(&store, "env", "__indirect_function_table", local_table)?;

    // 3. Instantiate
    let instance = instance_linker.instantiate(&mut *store, module)?;

    // Run constructors
    if let Some(func) = instance.get_func(&mut *store, "__wasm_call_ctors") {
        func.typed::<(), ()>(&mut *store)?.call(&mut *store, ())?;
    }

    Ok(instance)
}

fn main() -> Result<()> {
    println!("🚀 [HOST] Booting ECS Engine (Pro Architecture)...");

    let mut config = Config::new();
    config.wasm_threads(true);
    let engine = Engine::new(&config)?;

    // Zero-Copy Shared Memory
    let memory = SharedMemory::new(&engine, MemoryType::shared(256, 256))?;

    // Setup Allocator
    let heap_ptr = unsafe { memory.data().as_ptr().add(HEAP_START_OFFSET) };
    let param = BuddyAllocParam::new(heap_ptr as *const u8, HEAP_SIZE, 16);
    let allocator = unsafe { BuddyAlloc::new(param) };

    let host_state = HostState {
        instances: HashMap::new(),
        shared_memory: memory.clone(),
        next_memory_offset: 1024 * 1024,
        next_stack_offset: 65536,
        heap_allocator: Arc::new(Mutex::new(SystemAllocator(allocator))),
    };

    let mut store = Store::new(&engine, host_state);
    let mut linker = Linker::new(&engine);
    linker.allow_shadowing(true);

    // Register Host Functions
    linker.define(&store, "env", "memory", memory.clone())?;
    linker.func_wrap("env", "host_print", host_print)?;
    linker.func_wrap("env", "host_alloc", host_alloc)?;
    linker.func_wrap("env", "host_dealloc", host_dealloc)?;
    linker.func_wrap("env", "host_link_call", host_link_call)?;

    // Build paths - make sure these match your Cargo workspace output
    // Correct (assumes running from workspace root)
    let module_core = Module::from_file(
        &engine,
        "target/wasm32-unknown-unknown/release/ecs_core.wasm",
    )?;
    let module_game = Module::from_file(
        &engine,
        "target/wasm32-unknown-unknown/release/my_game.wasm",
    )?;

    // 1. Load Core
    let instance_core = instantiate_plugin(&linker, &mut store, &module_core, "Core")?;
    store
        .data_mut()
        .instances
        .insert("Core".to_string(), instance_core.clone());

    if let Some(func) = instance_core.get_func(&mut store, "setup_panic_hook") {
        func.typed::<(), ()>(&mut store)?.call(&mut store, ())?;
    }

    // 2. Link Core Exports (Optional if using mostly linker, but good for spawn_entity)
    let core_exports = [
        "spawn_entity",
        "register_component",
        "add_component",
        "query_archetypes",
        "get_table_column",
        "rebuild_schedule",
        "run_schedule",
    ];
    for name in core_exports {
        if let Some(func) = instance_core.get_func(&mut store, name) {
            linker.define(&store, "env", name, func)?;
        }
    }

    // 3. Load Game
    let instance_game = instantiate_plugin(&linker, &mut store, &module_game, "Game")?;
    store
        .data_mut()
        .instances
        .insert("Game".to_string(), instance_game.clone());

    println!("⚡ [HOST] Init Game...");
    instance_game
        .get_typed_func::<(), ()>(&mut store, "init")?
        .call(&mut store, ())?;

    println!("⚙️ [HOST] Rebuilding Schedule...");
    instance_core
        .get_typed_func::<(), ()>(&mut store, "rebuild_schedule")?
        .call(&mut store, ())?;

    println!("🔄 [HOST] Running Loop...");
    let run_fn = instance_core.get_typed_func::<(), ()>(&mut store, "run_schedule")?;

    for i in 1..=3 {
        run_fn.call(&mut store, ())?;
        println!("   Frame {} cycle complete.", i);
    }

    Ok(())
}
