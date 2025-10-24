use wasmtime::{Engine, Store, Module, Instance, Memory, Func, Val};
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum HostError {
    #[error("Wasm function not found: {0}")]
    FuncNotFound(String),
    #[error("Memory not found in plugin instance")]
    MemoryNotFound,
    #[error("Wasm call failed: {0}")]
    WasmCallFailed(#[from] anyhow::Error),
    #[error("UTF-8 conversion failed: {0}")]
    Utf8Error(#[from] std::string::FromUtf8Error),
    #[error("JSON deserialization failed: {0}")]
    JsonError(#[from] serde_json::Error),
}

/// Simple representation of a plugin
struct Plugin {
    instance: Instance,
    memory: Memory,
    funcs: HashMap<String, Func>,
}

/// The host orchestrates plugins and provides a message-bus-like TypedFunc registry
struct Host {
    store: Store<()>,
    plugins: HashMap<String, Plugin>,
    /// message bus: functions that expect typed input/output
    typed_funcs: HashMap<String, wasmtime::TypedFunc<(i32, i32), i32>>,
}

impl Host {
    pub fn new(engine: &Engine) -> Self {
        Self {
            store: Store::new(engine, ()),
            plugins: HashMap::new(),
            typed_funcs: HashMap::new(),
        }
    }

    /// Load a plugin from a wasm file
    pub fn load_plugin(&mut self, name: &str, path: &str) -> Result<(), HostError> {
        let module = Module::from_file(self.store.engine(), path)?;
        let instance = Instance::new(&mut self.store, &module, &[])?;
        let memory = instance
            .get_memory(&mut self.store, "memory")
            .ok_or(HostError::MemoryNotFound)?;
        
        // Collect all exported Funcs
        let mut func_map = HashMap::new();
        for export in module.exports() {
            if let wasmtime::ExternType::Func(_) = export.ty() {
                if let Some(f) = instance.get_func(&mut self.store, export.name()) {
                    func_map.insert(export.name().to_string(), f);
                }
            }
        }

        self.plugins.insert(
            name.to_string(),
            Plugin {
                instance,
                memory,
                funcs: func_map,
            },
        );

        Ok(())
    }

    /// Helper to read memory from a plugin
    fn read_memory(&mut self, plugin_name: &str, ptr: i32, len: i32) -> Result<Vec<u8>, HostError> {
        let plugin = self.plugins.get(plugin_name).ok_or(HostError::MemoryNotFound)?;
        let data = plugin.memory.data(&self.store);
        Ok(data[ptr as usize..(ptr + len) as usize].to_vec())
    }

    /// Register a message-bus function
    pub fn register_typed_func(&mut self, name: &str, func: wasmtime::TypedFunc<(i32, i32), i32>) {
        self.typed_funcs.insert(name.to_string(), func);
    }

    /// Call a message-bus function
    pub fn call_typed_func(&mut self, name: &str, arg1: i32, arg2: i32) -> Result<i32, HostError> {
        let f = self.typed_funcs.get(name).ok_or(HostError::FuncNotFound(name.to_string()))?;
        Ok(f.call(&mut self.store, (arg1, arg2))?)
    }
}

fn main() -> Result<(), HostError> {
    let engine = Engine::default();
    let mut host = Host::new(&engine);

    // --- Load plugins ---
    host.load_plugin("todoapp", "todoapp.wasm")?;
    host.load_plugin("reporter", "reporter.wasm")?;

    // --- Step 1: call pending_tasks() on todoapp ---
    let plugin_a = host.plugins.get("todoapp").unwrap();
    let pending_func = plugin_a
        .funcs
        .get("pending_tasks")
        .ok_or(HostError::FuncNotFound("pending_tasks".to_string()))?;
    
    let mut results = [Val::I32(0), Val::I32(0)];
    pending_func.call(&mut host.store, &[], &mut results)?;

    let ptr = results[0].unwrap_i32();
    let len = results[1].unwrap_i32();

    let json_bytes = host.read_memory("todoapp", ptr, len)?;
    let json_str = String::from_utf8(json_bytes)?;
    let pending: Value = serde_json::from_str(&json_str)?;
    println!("Pending tasks from todoapp: {}", json_str);

    // --- Step 2: allocate memory in reporter ---
    let plugin_b = host.plugins.get("reporter").unwrap();
    let alloc_func = plugin_b
        .funcs
        .get("allocate")
        .ok_or(HostError::FuncNotFound("allocate".to_string()))?;
    let mut alloc_result = [Val::I32(0)];
    alloc_func.call(&mut host.store, &[Val::I32(len)], &mut alloc_result)?;
    let ptr_b = alloc_result[0].unwrap_i32();

    // --- Step 3: write data into reporter memory ---
    plugin_b.memory.write(&mut host.store, ptr_b as usize, json_str.as_bytes())?;

    // --- Step 4: call reporter's handle_tasks ---
    let handle_func = plugin_b
        .funcs
        .get("handle_tasks")
        .ok_or(HostError::FuncNotFound("handle_tasks".to_string()))?;
    handle_func.call(&mut host.store, &[Val::I32(ptr_b), Val::I32(len)], &mut [])?;

    Ok(())
}
