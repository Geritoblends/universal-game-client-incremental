use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use wasmtime::*;

// --- CONFIGURATION ---
const WASM_PAGE_SIZE: u64 = 65536;
const GROWTH_CHUNK_SIZE: u64 = 80;
const HEAP_START_ADDR: u32 = 10 * 1024 * 1024;

// --- HEAP ALLOCATOR ---
#[derive(Debug, Clone, Copy)]
struct FreeBlock {
    addr: u32,
    size: u32,
}

pub struct HostHeap {
    free_blocks: Vec<FreeBlock>,
}
impl HostHeap {
    fn new() -> Self {
        Self {
            free_blocks: Vec::new(),
        }
    }
    fn coalesce(&mut self) {
        if self.free_blocks.is_empty() {
            return;
        }
        self.free_blocks.sort_by_key(|b| b.addr);
        let mut i = 0;
        while i < self.free_blocks.len() - 1 {
            let current = self.free_blocks[i];
            let next = self.free_blocks[i + 1];
            if current.addr + current.size == next.addr {
                self.free_blocks[i].size += next.size;
                self.free_blocks.remove(i + 1);
            } else {
                i += 1;
            }
        }
    }
    fn alloc(&mut self, size: u32) -> Option<u32> {
        if let Some(pos) = self.free_blocks.iter().position(|b| b.size >= size) {
            let block = self.free_blocks[pos];
            if block.size == size {
                self.free_blocks.remove(pos);
                Some(block.addr)
            } else {
                let ret_addr = block.addr;
                self.free_blocks[pos].addr += size;
                self.free_blocks[pos].size -= size;
                Some(ret_addr)
            }
        } else {
            None
        }
    }
    fn dealloc(&mut self, ptr: u32, size: u32) {
        self.free_blocks.push(FreeBlock { addr: ptr, size });
        self.coalesce();
    }
}

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

        // --- HOST LINK CALL ---
        linker.func_wrap(
            "env",
            "host_link_call",
            |mut c: Caller<'_, HostState>,
             tm_p: i32,
             tm_l: i32,
             tf_p: i32,
             tf_l: i32,
             local_fn_idx: i32,
             p_p: i32,
             p_l: i32|
             -> Result<()> {
                let (target_mod, target_func) = {
                    let mem = c.data().shared_memory.data();
                    let base = mem.as_ptr() as *const u8;
                    unsafe {
                        (
                            String::from_utf8_lossy(std::slice::from_raw_parts(
                                base.add(tm_p as usize),
                                tm_l as usize,
                            ))
                            .to_string(),
                            String::from_utf8_lossy(std::slice::from_raw_parts(
                                base.add(tf_p as usize),
                                tf_l as usize,
                            ))
                            .to_string(),
                        )
                    }
                };

                let caller_table = c
                    .data()
                    .tables
                    .get("Game")
                    .ok_or(anyhow!("Caller table 'Game' not found"))?
                    .clone();

                let func = *caller_table
                    .get(&mut c, local_fn_idx as u32)
                    .ok_or(anyhow!("Index {} OOB in Game table", local_fn_idx))?
                    .unwrap_func()
                    .ok_or(anyhow!("Function is null"))?;

                let core_table = c
                    .data()
                    .tables
                    .get(&target_mod)
                    .ok_or(anyhow!("Target table '{}' not found", target_mod))?
                    .clone();

                let new_idx = core_table.size(&mut c);
                core_table.grow(&mut c, 1, Ref::Func(Some(func)))?;

                println!(
                    "🔗 [HOST] Injected Game::Fn({}) -> {}::Table[{}]",
                    local_fn_idx, target_mod, new_idx
                );

                let core_instance = c
                    .data()
                    .instances
                    .get(&target_mod)
                    .ok_or(anyhow!("Target instance '{}' not found", target_mod))?
                    .clone();

                let hook = core_instance
                    .get_func(&mut c, &target_func)
                    .ok_or(anyhow!("Hook '{}' not found", target_func))?;

                let params = vec![Val::I32(new_idx as i32), Val::I32(p_p), Val::I32(p_l)];
                hook.call(&mut c, &params, &mut [])?;

                Ok(())
            },
        )?;

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

        // --- AUTO-EXPORT FIX ---
        // Iterate over everything this module exports (e.g., Core exports `query_archetypes`)
        // and register it in the MAIN linker under "env".
        // This allows subsequent modules (Game) to find it.
        let exports: Vec<(String, Extern)> = instance
            .exports(&mut self.store)
            .map(|e| (e.name().to_string(), e.into_extern()))
            .collect();

        for (export_name, export_val) in exports {
            // We define it in `self.linker` so the NEXT call to `prepare_env` picks it up.
            // We ignore errors (like re-defining "memory" or "init") silently.
            let _ = self
                .linker
                .define(&self.store, "env", &export_name, export_val);
        }

        if let Some(func) = instance.get_func(&mut self.store, "__wasm_call_ctors") {
            func.typed::<(), ()>(&mut self.store)?
                .call(&mut self.store, ())?;
        }
        if let Some(func) = instance.get_func(&mut self.store, "init") {
            func.typed::<(), ()>(&mut self.store)?
                .call(&mut self.store, ())?;
        }

        self.store
            .data_mut()
            .instances
            .insert(name.to_string(), instance.clone());
        Ok(instance)
    }

    fn prepare_env(&mut self, name: &str) -> Result<Linker<HostState>> {
        let current_mem_offset = self.store.data().next_memory_offset;
        let stack_ptr = self.store.data().next_stack_offset;

        self.store.data_mut().next_memory_offset += 512 * 1024;
        self.store.data_mut().next_stack_offset += 128 * 1024;

        // Clone the MAIN linker (which now contains exports from previous modules!)
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
