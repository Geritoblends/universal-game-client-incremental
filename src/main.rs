use wasmtime::{Instance, Config, Engine, Store}

#[derive(ThisError)]
enum Error {
    #[error("Wasmtime Error: {0}")]
    Wasmtime(#[from] wasmtime::Error),
}

fn main() -> Result<(), Error> {
    let engine = Engine::default();
    let mut store = Store::new(&engine, ());

    let core_module = Module::from_file(&engine, "core.wasm")?;
    let client_module = Module::from_file(&engine, "client.wasm")?;

    let linker = Linker::new(&engine);
    let core = linker.instantiate(&mut store, &core_module)?;
    let client = linker.instantiate(&mut store, &client_module)?;

    

    
