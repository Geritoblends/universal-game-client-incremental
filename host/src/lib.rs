pub mod allocator;
use allocator::*;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use wasmtime::{
    Caller, Config, Engine, Extern, Func, Global, GlobalType, Instance, Linker, MemoryType, Module,
    Mutability, Ref, RefType, SharedMemory, Store, Table, TableType, Val, ValType,
};

// --- CONFIGURATION ---
const WASM_PAGE_SIZE: u64 = 65536;
const GROWTH_CHUNK_SIZE: u64 = 80;
const HEAP_START_ADDR: u32 = 10 * 1024 * 1024;

// --- HOST STATE ---
#[derive(Clone)]
pub struct HostState {
    pub instances: HashMap<String, Instance>,
    pub tables: HashMap<String, Table>,
    pub shared_memory: SharedMemory,
    pub next_memory_offset: i32,
    pub next_stack_offset: i32,
    pub heap: Arc<Mutex<HostHeap>>,
}

pub struct BlindHost {
    pub engine: Engine,
    pub store: Store<HostState>,
    pub linker: Linker<HostState>,
}

impl BlindHost {
    pub fn new() -> Result<Self> {
        let mut config = Config::new();
        config.wasm_threads(true);
        let engine = Engine::new(&config)?;

        let memory = SharedMemory::new(&engine, MemoryType::shared(1000, 16384))?;

        let initial_state = HostState {
            instances: HashMap::new(),
            tables: HashMap::new(),
            shared_memory: memory.clone(),
            next_memory_offset: 1024,
            next_stack_offset: 5 * 1024 * 1024,
            heap: Arc::new(Mutex::new(HostHeap::new())),
        };

        let mut store = Store::new(&engine, initial_state);
        let mut linker = Linker::new(&engine);
        linker.allow_shadowing(true);

        linker.define(&store, "env", "memory", memory)?;
        linker.func_wrap("env", "host_print", host_print)?;
        linker.func_wrap("env", "host_alloc", host_alloc)?;
        linker.func_wrap("env", "host_dealloc", host_dealloc)?;

        Ok(Self {
            engine,
            store,
            linker,
        })
    }

    pub fn load_plugin(&mut self, name: &str, wasm_bytes: &[u8]) -> Result<Instance> {
        println!("📦 [HOST] Loading Plugin: {}", name);
        let module = Module::new(&self.engine, wasm_bytes)?;

        let instance_linker = self.prepare_env(name)?;

        let instance = instance_linker.instantiate(&mut self.store, &module)?;

        // --- 1. REGISTER INSTANCE IMMEDIATELY ---
        // We must do this NOW so that when `init` runs, the Host knows who "Game" is.
        self.store
            .data_mut()
            .instances
            .insert(name.to_string(), instance.clone());

        // --- 2. AUTO-EXPORT FIX (unchanged) ---
        let exports: Vec<(String, Extern)> = instance
            .exports(&mut self.store)
            .map(|e| (e.name().to_string(), e.into_extern()))
            .collect();

        for (export_name, export_val) in exports {
            let _ = self
                .linker
                .define(&self.store, "env", &export_name, export_val);
        }

        // --- 3. RUN CONSTRUCTORS & INIT ---
        if let Some(func) = instance.get_func(&mut self.store, "__wasm_call_ctors") {
            func.typed::<(), ()>(&mut self.store)?
                .call(&mut self.store, ())?;
        }

        // When this runs now, `host_link_call` will successfully find "Game" in the map!
        if let Some(func) = instance.get_func(&mut self.store, "init") {
            func.typed::<(), ()>(&mut self.store)?
                .call(&mut self.store, ())?;
        }

        Ok(instance)
    }

    fn prepare_env(&mut self, name: &str) -> Result<Linker<HostState>> {
        let current_mem_offset = self.store.data().next_memory_offset;
        let stack_ptr = self.store.data().next_stack_offset;

        self.store.data_mut().next_memory_offset += 512 * 1024;
        self.store.data_mut().next_stack_offset += 128 * 1024;

        let mut linker = self.linker.clone();

        let table = Table::new(
            &mut self.store,
            TableType::new(RefType::FUNCREF, 1024, None),
            Ref::Func(None),
        )?;

        linker.define(&self.store, "env", "__indirect_function_table", table)?;
        self.store.data_mut().tables.insert(name.to_string(), table);

        let g_mem = Global::new(
            &mut self.store,
            GlobalType::new(ValType::I32, Mutability::Const),
            Val::I32(current_mem_offset),
        )?;
        let g_stk = Global::new(
            &mut self.store,
            GlobalType::new(ValType::I32, Mutability::Var),
            Val::I32(stack_ptr),
        )?;
        let g_tbl_base = Global::new(
            &mut self.store,
            GlobalType::new(ValType::I32, Mutability::Const),
            Val::I32(0),
        )?;

        linker.define(&self.store, "env", "__memory_base", g_mem)?;
        linker.define(&self.store, "env", "__stack_pointer", g_stk)?;
        linker.define(&self.store, "env", "__table_base", g_tbl_base)?;

        // --- ✅ CONTEXT-AWARE HOST LINK CALL ---
        // Inside prepare_env(name: &str) ...

        let caller_name = name.to_string(); // e.g., "Core"

        linker.func_wrap(
            "env",
            "host_link_call",
            move |mut c: Caller<'_, HostState>,
                  provider_mod_ptr: i32,
                  provider_mod_len: i32,
                  provider_fn_ptr: i32,
                  provider_fn_len: i32|
                  -> Result<i32> {
                // Returns Index!

                // 1. Read the String arguments from Shared Memory
                // (Since memory is shared, Core provided pointers to strings written by Game)
                let (provider_mod, provider_func) = {
                    let mem = c.data().shared_memory.data();
                    let base = mem.as_ptr() as *const u8;
                    unsafe {
                        (
                            String::from_utf8_lossy(std::slice::from_raw_parts(
                                base.add(provider_mod_ptr as usize),
                                provider_mod_len as usize,
                            ))
                            .to_string(),
                            String::from_utf8_lossy(std::slice::from_raw_parts(
                                base.add(provider_fn_ptr as usize),
                                provider_fn_len as usize,
                            ))
                            .to_string(),
                        )
                    }
                };

                // 2. Find the Provider's Instance (e.g., "Game")
                println!("Available instances: {:?}", c.data().instances.keys());
                let provider_instance = c
                    .data()
                    .instances
                    .get(&provider_mod)
                    .ok_or(anyhow!("Provider module '{}' not loaded", provider_mod))?
                    .clone();

                // 3. Get the Function (using the Named Export!)
                let func = provider_instance
                    .get_func(&mut c, &provider_func)
                    .ok_or(anyhow!(
                        "Export '{}' not found in '{}'",
                        provider_func,
                        provider_mod
                    ))?;

                // 4. Get the Caller's Table (e.g., "Core")
                // We inject the function into the person who asked for it.
                let caller_table = c
                    .data()
                    .tables
                    .get(&caller_name)
                    .ok_or(anyhow!("Caller table '{}' not found", caller_name))?
                    .clone();

                // 5. Inject and return Index
                let new_idx = caller_table.size(&mut c);
                caller_table.grow(&mut c, 1, Ref::Func(Some(func)))?;

                println!(
                    "🔗 [HOST] Linked {}::{} into {}::Table[{}]",
                    provider_mod, provider_func, caller_name, new_idx
                );

                Ok(new_idx as i32)
            },
        )?;

        Ok(linker)
    }

    pub fn get_func(&mut self, module_name: &str, func_name: &str) -> Result<Func> {
        let instance = self
            .store
            .data()
            .instances
            .get(module_name)
            .ok_or(anyhow!("Instance '{}' not loaded", module_name))?
            .clone();

        instance.get_func(&mut self.store, func_name).ok_or(anyhow!(
            "Function '{}' not found in module '{}'",
            func_name,
            module_name
        ))
    }
}

// --- ALLOCATOR IMPL ---
fn host_alloc(caller: Caller<'_, HostState>, size: i32) -> i32 {
    let size = (size as u32 + 7) & !7;
    let memory = caller.data().shared_memory.clone();
    let mut heap = caller.data().heap.lock().unwrap();

    if let Some(addr) = heap.alloc(size) {
        return addr as i32;
    }

    let current_mem_size = (memory.size() * WASM_PAGE_SIZE) as u64;
    let growth_start_addr =
        if heap.free_blocks.is_empty() && current_mem_size < HEAP_START_ADDR as u64 {
            HEAP_START_ADDR
        } else {
            current_mem_size as u32
        };

    let required_growth = std::cmp::max(
        GROWTH_CHUNK_SIZE,
        (size as u64 + WASM_PAGE_SIZE - 1) / WASM_PAGE_SIZE,
    );

    if memory.grow(required_growth).is_err() {
        return 0;
    }

    let new_block_size = (required_growth * WASM_PAGE_SIZE) as u32;
    heap.dealloc(growth_start_addr, new_block_size);

    heap.alloc(size).unwrap_or(0) as i32
}

fn host_dealloc(caller: Caller<'_, HostState>, ptr: i32, size: i32) {
    if ptr == 0 {
        return;
    }
    let ptr = ptr as u32;
    let size = (size as u32 + 7) & !7;
    caller.data().heap.lock().unwrap().dealloc(ptr, size);
}

fn host_print(caller: Caller<'_, HostState>, ptr: i32, len: i32) -> Result<()> {
    let mem = caller.data().shared_memory.data();
    if ptr < 0 || (ptr as usize + len as usize) > mem.len() {
        return Ok(());
    }

    let base_ptr = mem.as_ptr() as *const u8;

    let s = unsafe {
        std::str::from_utf8_unchecked(std::slice::from_raw_parts(
            base_ptr.add(ptr as usize),
            len as usize,
        ))
    };
    println!("{}", s);
    Ok(())
}
