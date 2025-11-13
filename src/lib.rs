use wasmtime::*;
use std::collections::HashMap;

pub struct HostState {
    pub instances: HashMap<String, Instance>,
}

pub fn call(
    mut caller: Caller<'_, HostState>,
    instance_id_ptr: i32,
    instance_id_len: i32,
    func_name_ptr: i32,
    func_name_len: i32,
    payload_ptr: i32,
    payload_len: i32,
) -> Result<(i32, i32), Trap> {
    let mem = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| Trap::i32_exit(1))?;
    
    let mem_data = mem.data(&caller);
    
    let instance_id_bytes = &mem_data[instance_id_ptr as usize..(instance_id_ptr + instance_id_len) as usize];
    let func_name_bytes = &mem_data[func_name_ptr as usize..(func_name_ptr + func_name_len) as usize];
    
    let instance_id = String::from_utf8_lossy(instance_id_bytes).to_string();
    let func_name = String::from_utf8_lossy(func_name_bytes).to_string();
    
    let instance = caller
        .data()
        .instances
        .get(&instance_id)
        .ok_or_else(|| Trap::i32_exit(1))?;
    
    let func = instance
        .get_export(&mut caller, &func_name)
        .and_then(|e| e.into_func())
        .ok_or_else(|| Trap::i32_exit(1))?;
    
    let typed = func
        .typed::<(i32, i32), (i32, i32)>(&caller)
        .map_err(|_| Trap::i32_exit(1))?;
    
    // The magic: these pointers work because both modules share memory!
    typed.call(&mut caller, (payload_ptr, payload_len))
}

pub fn send_to_server(
    mut caller: Caller<'_, HostState>,
    message_ptr: i32,
    message_len: i32,
) -> Result<(), Trap> {
    let mem = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| Trap::i32_exit(1))?;
    
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
    payload_len: i32,
) -> Result<(), Trap> {
    let mem = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| Trap::i32_exit(1))?;
    
    let mem_data = mem.data(&caller);
    
    let instance_id_bytes = &mem_data[instance_id_ptr as usize..(instance_id_ptr + instance_id_len) as usize];
    let func_name_bytes = &mem_data[func_ptr as usize..(func_ptr + func_len) as usize];
    
    let instance_id = String::from_utf8_lossy(instance_id_bytes).to_string();
    let func_name = String::from_utf8_lossy(func_name_bytes).to_string();
    
    let instance = caller
        .data()
        .instances
        .get(&instance_id)
        .ok_or_else(|| Trap::i32_exit(1))?;
    
    let func = instance
        .get_export(&mut caller, &func_name)
        .and_then(|e| e.into_func())
        .ok_or_else(|| Trap::i32_exit(1))?;
    
    let typed = func
        .typed::<i32, ()>(&caller)
        .map_err(|_| Trap::i32_exit(1))?;
    
    typed.call(&mut caller, payload_ptr)?;
    Ok(())
}

pub fn setup_runtime() -> Result<(Store<HostState>, Instance, Instance), Box<dyn std::error::Error>> {
    let engine = Engine::default();
    let mut store = Store::new(&engine, HostState {
        instances: HashMap::new(),
    });
    
    // Create shared memory
    let memory_type = MemoryType::new(10, Some(100));
    let shared_memory = Memory::new(&mut store, memory_type)?;
    
    // Load and link tasksapp_core
    let module_core = Module::from_file(&engine, "target/wasm32-unknown-unknown/release/tasksapp_core.wasm")?;
    let mut linker_core = Linker::new(&engine);
    linker_core.define(&store, "env", "memory", shared_memory)?;
    linker_core.func_wrap("env", "call", call)?;
    linker_core.func_wrap("env", "send_to_server", send_to_server)?;
    linker_core.func_wrap("env", "fire_and_forget", fire_and_forget)?;
    let instance_core = linker_core.instantiate(&mut store, &module_core)?;
    
    // Load and link tasksapp_client with SAME memory
    let module_client = Module::from_file(&engine, "target/wasm32-unknown-unknown/release/tasksapp_client.wasm")?;
    let mut linker_client = Linker::new(&engine);
    linker_client.define(&store, "env", "memory", shared_memory)?;
    linker_client.func_wrap("env", "call", call)?;
    linker_client.func_wrap("env", "send_to_server", send_to_server)?;
    linker_client.func_wrap("env", "fire_and_forget", fire_and_forget)?;
    let instance_client = linker_client.instantiate(&mut store, &module_client)?;
    
    // Register instances
    store.data_mut().instances.insert("tasksapp_core".to_string(), instance_core.clone());
    store.data_mut().instances.insert("tasksapp_client".to_string(), instance_client.clone());
    
    Ok((store, instance_core, instance_client))
}
