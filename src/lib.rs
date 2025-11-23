// Added `anyhow` imports
use anyhow::{Result, anyhow};
use std::cell::UnsafeCell;
use std::collections::HashMap;
use wasmtime::*;

pub struct HostState {
    pub instances: HashMap<String, Instance>,
}

pub fn host_print(
    mut caller: Caller<'_, HostState>,
    message_ptr: i32,
    message_len: i32,
) -> Result<()> {
    let mem = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| anyhow!("'memory' export not found or not a memory"))?;

    let mem_data = mem.data(&mut caller);
    let message_bytes = &mem_data[message_ptr as usize..(message_ptr + message_len) as usize];

    let message = String::from_utf8_lossy(message_bytes);
    println!("{}", message);

    Ok(())
}

pub fn call(
    mut caller: Caller<'_, HostState>,
    instance_id_ptr: i32,
    instance_id_len: i32,
    func_name_ptr: i32,
    func_name_len: i32,
    payload_ptr: i32,
    payload_len: i32,
) -> Result<i64> {
    // ← Returns i64 (packed i32, i32)

    // Read from shared memory
    let mem = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| anyhow!("'memory' export not found or not a memory"))?;

    let mem_data = mem.data(&mut caller);

    let instance_id_bytes =
        &mem_data[instance_id_ptr as usize..(instance_id_ptr + instance_id_len) as usize];
    let func_name_bytes =
        &mem_data[func_name_ptr as usize..(func_name_ptr + func_name_len) as usize];

    let instance_id = String::from_utf8_lossy(instance_id_bytes).to_string();
    let func_name = String::from_utf8_lossy(func_name_bytes).to_string();

    let instance = caller
        .data()
        .instances
        .get(&instance_id)
        .ok_or_else(|| anyhow!("instance '{}' not found in host state", instance_id))?
        .clone();

    let func = instance
        .get_export(&mut caller, &func_name)
        .and_then(|e| e.into_func())
        .ok_or_else(|| {
            anyhow!(
                "function '{}' not found in instance '{}'",
                func_name,
                instance_id
            )
        })?;

    // FIX: Use the correct signature - (i32, i32) -> i64
    let typed = func
        .typed::<(i32, i32), i64>(&caller) // ✅ Matches your core module
        .map_err(|_| anyhow!("function '{}' has incorrect signature", func_name))?;

    // Call the function - it returns i64 directly
    let packed_result = typed.call(&mut caller, (payload_ptr, payload_len))?;

    Ok(packed_result)
}

pub fn send_to_server(
    mut caller: Caller<'_, HostState>,
    message_ptr: i32,
    message_len: i32,
) -> Result<()> {
    let mem = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| anyhow!("'memory' export not found or not a memory"))?;

    let mem_data = mem.data(&caller);
    let message_bytes = &mem_data[message_ptr as usize..(message_ptr + message_len) as usize];

    println!("📤 Server received: {} bytes", message_bytes.len());
    println!("   Message: {}", String::from_utf8_lossy(message_bytes));
    Ok(())
}

pub fn fire_and_forget(
    mut caller: Caller<'_, HostState>,
    instance_id_ptr: i32,
    instance_id_len: i32,
    func_ptr: i32,
    func_len: i32,
    payload_ptr: i32,
    _payload_len: i32, // <-- FIX: Added underscore for unused variable
) -> Result<()> {
    let mem = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| anyhow!("'memory' export not found or not a memory"))?;

    let mem_data = mem.data(&caller);

    let instance_id_bytes =
        &mem_data[instance_id_ptr as usize..(instance_id_ptr + instance_id_len) as usize];
    let func_name_bytes = &mem_data[func_ptr as usize..(func_ptr + func_len) as usize];

    let instance_id = String::from_utf8_lossy(instance_id_bytes).to_string();
    let func_name = String::from_utf8_lossy(func_name_bytes).to_string();

    let instance = caller
        .data()
        .instances
        .get(&instance_id)
        .ok_or_else(|| anyhow!("instance '{}' not found in host state", instance_id))?
        .clone(); // <-- FIX: Added clone to resolve borrow error

    let func = instance
        .get_export(&mut caller, &func_name)
        .and_then(|e| e.into_func())
        .ok_or_else(|| {
            anyhow!(
                "function '{}' not found in instance '{}'",
                func_name,
                instance_id
            )
        })?;

    let typed = func
        .typed::<i32, ()>(&caller)
        .map_err(|_| anyhow!("function '{}' has incorrect signature", func_name))?;

    typed.call(&mut caller, payload_ptr)?;
    Ok(())
}

pub fn setup_runtime() -> Result<(Store<HostState>, Instance, Instance)> {
    // 1. Configure the Engine to enable the WebAssembly Threads proposal
    let mut config = Config::new();
    config.wasm_threads(true); // REQUIRED for SharedMemory support

    // 2. Create the Engine using the configured Config
    let engine = Engine::new(&config)?;

    // 3. Create the Store using the configured Engine
    let mut store = Store::new(
        &engine,
        HostState {
            instances: HashMap::new(),
        },
    );

    // 4. Create shared memory
    // MemoryType::shared takes two u32 values (min pages, max pages). No Option required.
    let memory_type = MemoryType::shared(17, 20);

    // SharedMemory::new requires a reference to the Engine, not the Store.
    let memory = SharedMemory::new(&engine, memory_type)?;

    // Load and link tasksapp_core
    let module_core = Module::from_file(
        &engine,
        "plugins/tasksapp-core/target/wasm32-unknown-unknown/release/tasksapp_core.wasm",
    )?;
    let mut linker_core = Linker::new(&engine);
    // Note: Linker::define requires a mutable store reference if linking non-function items like memory
    linker_core.define(&mut store, "env", "memory", memory.clone())?;
    linker_core.func_wrap("env", "call", call)?;
    linker_core.func_wrap("env", "send_to_server", send_to_server)?;
    linker_core.func_wrap("env", "fire_and_forget", fire_and_forget)?;
    linker_core.func_wrap("env", "host_print", host_print)?;
    let instance_core = linker_core.instantiate(&mut store, &module_core)?;

    // Load and link tasksapp_client with SAME memory
    let module_client = Module::from_file(
        &engine,
        "plugins/tasksapp-client/target/wasm32-unknown-unknown/release/tasksapp_client.wasm",
    )?;
    let mut linker_client = Linker::new(&engine);
    // Link the SAME shared memory instance, using a clone
    linker_client.define(&mut store, "env", "memory", memory.clone())?;
    linker_client.func_wrap("env", "call", call)?;
    linker_client.func_wrap("env", "send_to_server", send_to_server)?;
    linker_client.func_wrap("env", "fire_and_forget", fire_and_forget)?;
    linker_client.func_wrap("env", "host_print", host_print)?;
    let instance_client = linker_client.instantiate(&mut store, &module_client)?;

    // Register instances
    store
        .data_mut()
        .instances
        .insert("tasksapp_core".to_string(), instance_core.clone());
    store
        .data_mut()
        .instances
        .insert("tasksapp_client".to_string(), instance_client.clone());

    Ok((store, instance_core, instance_client))
}
