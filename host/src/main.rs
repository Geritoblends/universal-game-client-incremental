use anyhow::Result;
use host::host::host_object::{BlindHost, BlindHostConfig};

fn main() -> Result<()> {
    let config = BlindHostConfig::default();
    let mut host = BlindHost::new(config)?;

    // --- 1. Load Modules (Release Mode) ---
    // Using "../target/wasm32-unknown-unknown/release/" based on your output

    println!("🔎 Loading WASM from release folder...");

    let core_path = "target/wasm32-unknown-unknown/release/ecs_core.wasm";
    // Check for 'my_game.wasm' since that's what appeared in your ls output
    let game_path = "target/wasm32-unknown-unknown/release/my_game.wasm";

    println!("📄 Reading Core: {}", core_path);
    let core_wasm = std::fs::read(core_path)?;
    host.load_plugin("Core", &core_wasm)?;

    println!("📄 Reading Game: {}", game_path);
    let game_wasm = std::fs::read(game_path)?;
    host.load_plugin("Game", &game_wasm)?;

    // --- 2. Run Init (Game) ---
    // println!("--- Running Game Init ---");
    // let init_fn = host.get_func("Game", "init")?;
    // let init_typed = init_fn.typed::<(), ()>(&host.store)?;
    // init_typed.call(&mut host.store, ())?;

    // --- 3. Rebuild Schedule (Core) ---
    println!("--- Rebuilding Schedule ---");
    let rebuild = host.get_func("Core", "rebuild_schedule")?;
    let rebuild_typed = rebuild.typed::<(), ()>(&host.store)?;
    rebuild_typed.call(&mut host.store, ())?;

    // --- 4. Run Loop ---
    println!("--- Starting Loop ---");
    let run = host.get_func("Core", "run_schedule")?;
    let run_typed = run.typed::<(), ()>(&host.store)?;

    for i in 0..5 {
        println!("\n[Frame {}]", i);
        run_typed.call(&mut host.store, ())?;
    }

    Ok(())
}
