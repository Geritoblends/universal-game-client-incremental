use super::caller_state::HostState;
use crate::allocator::HostHeap;
use crate::host_calls::allocator::{host_alloc, host_dealloc};
use crate::host_calls::print::host_print;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use wasmtime::{
    Caller, Config, Engine, Extern, Func, Global, GlobalType, Instance, Linker, MemoryType, Module,
    Mutability, Ref, RefType, SharedMemory, Store, Table, TableType, Val, ValType,
};

const DATA_REGION_START: i32 = 1024;
const STACK_REGION_START: i32 = 16 * 1024 * 1024;
const MODULE_DATA_ALLOWANCE: i32 = 1 * 1024 * 1024;
const MODULE_STACK_SIZE: i32 = 1 * 1024 * 1024;

pub struct BlindHostConfig {
    pub max_plugins: u32,
    pub data_allowance: i32,
    pub stack_size: i32,
}

impl BlindHostConfig {
    pub fn default() -> Self {
        Self {
            max_plugins: 16,
            data_allowance: 128 * 1024,
            stack_size: 1 * 1024 * 1024,
        }
    }

    pub fn slot_size(&self) -> i32 {
        let size = self.data_allowance + self.stack_size + 16;

        (size + 4095) & !4095
    }
}

pub struct BlindHost {
    pub engine: Engine,
    pub store: Store<HostState>,
    pub linker: Linker<HostState>,
}

impl BlindHost {
    pub fn new<F>(config: BlindHostConfig, setup_linker: F) -> Result<Self>
    where
        F: FnOnce(&mut Linker<HostState>, &mut Store<HostState>) -> Result<()>,
    {
        let mut wasm_config = Config::new();
        wasm_config.wasm_threads(true);
        let engine = Engine::new(&wasm_config)?;

        // --- 1. EXACT CALCULATION ---
        let slot_size = config.slot_size();
        let total_reserved_bytes = 1024 + (slot_size * (config.max_plugins as i32));

        // Align Heap Start to next 64KB Page (standard Wasm page alignment)
        let heap_start_address = (total_reserved_bytes + 65535) & !65535;

        // Convert our requirement to Wasm Pages (64KB each)
        let needed_pages = heap_start_address / 65536;

        // --- 2. THE SAFETY BUFFER ---
        // Rust's memory allocator (dlmalloc/wee_alloc) grabs a chunk of memory
        // immediately on startup to manage the heap.
        // 32 Pages = 2 MB. This is plenty for overhead but small enough to be efficient.
        let safety_buffer_pages = 256;

        let initial_pages = needed_pages + safety_buffer_pages;

        // println!("⚙️ [HOST] Memory Optimization:");
        // println!(
        //     "   ├── Reserved for Slots: {:.2} MB ({} Pages)",
        //     needed_pages as f32 * 64.0 / 1024.0,
        //     needed_pages
        // );
        // println!(
        //     "   ├── Runtime Buffer:     2.00 MB ({} Pages)",
        //     safety_buffer_pages
        // );
        // println!(
        //     "   └── Total Allocation:   {:.2} MB",
        //     initial_pages as f32 * 64.0 / 1024.0
        // );

        // --- 3. CREATE MEMORY ---
        let memory = SharedMemory::new(&engine, MemoryType::shared(initial_pages as u32, 16384))?;

        // --- 4. STATE SETUP (Same as before) ---
        let initial_state = HostState {
            instances: HashMap::new(),
            tables: HashMap::new(),
            shared_memory: memory.clone(),
            next_memory_offset: 1024,
            next_stack_offset: 0,
            slot_size,
            heap_start_address,
            data_size: config.data_allowance,
            heap: Arc::new(Mutex::new(HostHeap::new())),
        };

        let mut store = Store::new(&engine, initial_state);
        let mut linker = Linker::new(&engine);
        linker.allow_shadowing(true);

        linker.define(&store, "env", "memory", memory)?;
        linker.func_wrap("env", "host_print", host_print)?;
        linker.func_wrap("env", "host_alloc", host_alloc)?;
        linker.func_wrap("env", "host_dealloc", host_dealloc)?;

        setup_linker(&mut linker, &mut store)?;

        Ok(Self {
            engine,
            store,
            linker,
        })
    }

    // load_plugin remains exactly the same as your working version
    pub fn load_plugin(&mut self, name: &str, wasm_bytes: &[u8]) -> Result<Instance> {
        // println!("📦 [HOST] Loading Plugin: {}", name);
        let module = Module::new(&self.engine, wasm_bytes)?;
        let instance_linker = self.prepare_env(name)?;
        let instance = instance_linker.instantiate(&mut self.store, &module)?;

        self.store
            .data_mut()
            .instances
            .insert(name.to_string(), instance.clone());

        // Auto-Export
        let exports: Vec<(String, Extern)> = instance
            .exports(&mut self.store)
            .map(|e| (e.name().to_string(), e.into_extern()))
            .collect();

        for (export_name, export_val) in exports {
            let _ = self
                .linker
                .define(&self.store, "env", &export_name, export_val);
        }

        // Init
        if let Some(func) = instance.get_func(&mut self.store, "__wasm_call_ctors") {
            func.typed::<(), ()>(&mut self.store)?
                .call(&mut self.store, ())?;
        }
        if let Some(func) = instance.get_func(&mut self.store, "init") {
            func.typed::<(), ()>(&mut self.store)?
                .call(&mut self.store, ())?;
        }

        Ok(instance)
    }

    fn prepare_env(&mut self, name: &str) -> Result<Linker<HostState>> {
        let state = self.store.data();
        let slot_base = state.next_memory_offset;
        let slot_size = state.slot_size;
        let heap_limit = state.heap_start_address;

        // Safety Check
        if slot_base + slot_size > heap_limit {
            return Err(anyhow!("❌ Out of Module Slots!"));
        }

        let my_data_start = slot_base;
        let my_stack_top = slot_base + slot_size - 16;

        // Advance Pointers
        self.store.data_mut().next_memory_offset += slot_size;

        // println!("       ├── Slot Base:  {:#X}", slot_base);
        // println!("       └── Stack Top:  {:#X}", my_stack_top);

        let mut linker = self.linker.clone();

        // 1. Table
        let table = Table::new(
            &mut self.store,
            TableType::new(RefType::FUNCREF, 1024, None),
            Ref::Func(None),
        )?;
        linker.define(&self.store, "env", "__indirect_function_table", table)?;
        self.store.data_mut().tables.insert(name.to_string(), table);

        // 2. Globals (Created INDIVIDUALLY to satisfy Borrow Checker)
        let g_mem = Global::new(
            &mut self.store,
            GlobalType::new(ValType::I32, Mutability::Const),
            Val::I32(my_data_start),
        )?;
        linker.define(&self.store, "env", "__memory_base", g_mem)?;

        let g_stk = Global::new(
            &mut self.store,
            GlobalType::new(ValType::I32, Mutability::Var),
            Val::I32(my_stack_top),
        )?;
        linker.define(&self.store, "env", "__stack_pointer", g_stk)?;

        let g_tbl = Global::new(
            &mut self.store,
            GlobalType::new(ValType::I32, Mutability::Const),
            Val::I32(0),
        )?;
        linker.define(&self.store, "env", "__table_base", g_tbl)?;

        // 3. Host Link Call
        let caller_name = name.to_string();

        linker.func_wrap(
            "env",
            "host_link_call",
            move |mut c: Caller<'_, HostState>,
                  provider_mod_ptr: i32,
                  provider_mod_len: i32,
                  provider_fn_ptr: i32,
                  provider_fn_len: i32|
                  -> Result<i32> {
                // --- SAFE STRING READ ---
                // We access memory directly to replicate your working logic,
                // but we do it safely inside the closure.
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

                // Logic to find instance and function
                let provider_instance = c
                    .data()
                    .instances
                    .get(&provider_mod)
                    .ok_or(anyhow!("Provider '{}' not found", provider_mod))?
                    .clone();

                let func = provider_instance
                    .get_func(&mut c, &provider_func)
                    .ok_or(anyhow!("Export '{}' not found", provider_func))?;

                let caller_table = c
                    .data()
                    .tables
                    .get(&caller_name)
                    .ok_or(anyhow!("Table for '{}' not found", caller_name))?
                    .clone();

                let new_idx = caller_table.size(&mut c);
                caller_table.grow(&mut c, 1, Ref::Func(Some(func)))?;

                // println!(
                //     "🔗 [HOST] Linked {}::{} -> {}::Table[{}]",
                //     provider_mod, provider_func, caller_name, new_idx
                // );
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
            .ok_or(anyhow!("Instance not found"))?
            .clone();
        instance
            .get_func(&mut self.store, func_name)
            .ok_or(anyhow!("Function not found"))
    }

    pub fn read_mem(&mut self, ptr: i32, len: i32) -> Result<Vec<u8>> {
        // 1. Get the shared memory handle from the store data
        let memory = &self.store.data().shared_memory;

        // 2. Get the raw data (In Wasmtime 21+, this returns &[UnsafeCell<u8>])
        let data_cells = memory.data();

        // 3. SAFETY: Cast UnsafeCell<u8> to u8.
        // This is safe because our Host is single-threaded relative to the WASM execution.
        let data: &[u8] = unsafe {
            std::slice::from_raw_parts(data_cells.as_ptr() as *const u8, data_cells.len())
        };

        // 4. Perform bounds checking
        let start = ptr as usize;
        let end = start + len as usize;

        if end > data.len() {
            anyhow::bail!("Memory access out of bounds: {} > {}", end, data.len());
        }

        // 5. Copy the data
        Ok(data[start..end].to_vec())
    }

    pub fn write_mem(&mut self, ptr: i32, data: &[u8]) -> Result<()> {
        let memory = &self.store.data().shared_memory;
        let mem_cells = memory.data();

        // Safety: Cast to mutable u8 slice
        let mem_slice: &mut [u8] = unsafe {
            std::slice::from_raw_parts_mut(mem_cells.as_ptr() as *mut u8, mem_cells.len())
        };

        let start = ptr as usize;
        let end = start + data.len();

        if end > mem_slice.len() {
            anyhow::bail!("Memory write out of bounds");
        }

        mem_slice[start..end].copy_from_slice(data);
        Ok(())
    }
}
