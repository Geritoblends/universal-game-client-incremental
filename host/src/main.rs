use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{prelude::*, widgets::*};
use std::io::stdout;
use std::time::{Duration, Instant};
use wasmtime::TypedFunc;

// Internal crate imports
pub mod allocator;
pub mod host;
pub mod host_calls;

use host::host_object::{BlindHost, BlindHostConfig};
use grid_protocol::{
    GridCell, GridInput, 
    INPUT_KEY, INPUT_NONE, 
    KEY_ENTER, KEY_ESC, KEY_BACKSPACE, KEY_LEFT, KEY_RIGHT, KEY_UP, KEY_DOWN, KEY_DELETE, KEY_TAB,
    MOD_SHIFT, MOD_CTRL, MOD_ALT
};

// Helper to map keys from Crossterm to GridInput
fn map_key(event: KeyEvent) -> GridInput {
    let mut input = GridInput {
        input_type: INPUT_KEY,
        key_code: 0,
        modifiers: 0,
        padding: [0; 3],
    };

    // Map Modifiers
    if event.modifiers.contains(KeyModifiers::SHIFT) { input.modifiers |= MOD_SHIFT; }
    if event.modifiers.contains(KeyModifiers::CONTROL) { input.modifiers |= MOD_CTRL; }
    if event.modifiers.contains(KeyModifiers::ALT) { input.modifiers |= MOD_ALT; }

    // Map Code
    match event.code {
        KeyCode::Char(c) => input.key_code = c as u32,
        KeyCode::Enter => input.key_code = KEY_ENTER,
        KeyCode::Esc => input.key_code = KEY_ESC,
        KeyCode::Backspace => input.key_code = KEY_BACKSPACE,
        KeyCode::Left => input.key_code = KEY_LEFT,
        KeyCode::Right => input.key_code = KEY_RIGHT,
        KeyCode::Up => input.key_code = KEY_UP,
        KeyCode::Down => input.key_code = KEY_DOWN,
        KeyCode::Delete => input.key_code = KEY_DELETE,
        KeyCode::Tab => input.key_code = KEY_TAB,
        _ => {
            input.input_type = INPUT_NONE; // Ignore others for now
        }
    }
    input
}

fn main() -> Result<()> {
    // 1. Config & Host Setup
    let config = BlindHostConfig::default();
    
    // We don't need any special host calls for this MVP, but we must pass a linker setup closure
    let mut host = BlindHost::new(config, |_, _| Ok(()))?;

    // 2. Initialize Shared Heap
    // The HostHeap starts empty. We must give it the free memory region to manage.
    {
        let data = host.store.data();
        let heap_start = data.heap_start_address as u32;
        // SharedMemory len is in bytes
        let mem_size = data.shared_memory.data().len() as u32;
        
        let mut heap = data.heap.lock().unwrap();
        // Initialize the heap with the remaining free memory block
        if heap.free_blocks.is_empty() {
            heap.dealloc(heap_start, mem_size - heap_start);
        }
    }

    // 3. Load the Driver Plugin
    // We expect the WASM to be built in the target directory
    let wasm_path = "target/wasm32-unknown-unknown/release/grid_driver.wasm";
    if !std::path::Path::new(wasm_path).exists() {
        // Fallback or Error
        eprintln!("❌ Error: WASM driver not found at '{}'", wasm_path);
        eprintln!("   Please run: cargo build -p grid-driver --target wasm32-unknown-unknown --release");
        return Ok(());
    }
    
    let wasm_bytes = std::fs::read(wasm_path).context("Failed to read grid_driver.wasm")?;
    host.load_plugin("grid-driver", &wasm_bytes)?;

    // 4. Bind Exports
    // Typed functions for performance and type safety
    let tick_fn: TypedFunc<(f32,), ()> = host.get_func("grid-driver", "tick")?.typed(&host.store)?;
    let set_input_fn: TypedFunc<(i32,), ()> = host.get_func("grid-driver", "set_input")?.typed(&host.store)?;
    let set_tickrate_fn: TypedFunc<(f32,), ()> = host.get_func("grid-driver", "set_tickrate")?.typed(&host.store)?;
    let get_dims_fn: TypedFunc<(), i64> = host.get_func("grid-driver", "get_grid_dimensions")?.typed(&host.store)?;
    let get_ptr_fn: TypedFunc<(), i32> = host.get_func("grid-driver", "get_grid_ptr")?.typed(&host.store)?;

    // 5. Allocate Input Buffer in Shared Memory
    // The driver reads from this pointer. We write to it.
    let input_layout = std::alloc::Layout::new::<GridInput>();
    let input_ptr = {
        let mut heap = host.store.data().heap.lock().unwrap();
        // alloc returns Option<u32>
        heap.alloc(input_layout.size() as u32)
            .ok_or(anyhow::anyhow!("Failed to allocate input buffer in SharedMemory"))? as i32
    };

    // 6. TUI Initialization
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 7. Main Loop
    let mut tick_rate = 0.0; // Hz. 0.0 means "input driven"
    
    // Notify driver of initial tickrate
    set_tickrate_fn.call(&mut host.store, (tick_rate,))?;

    let mut last_tick = Instant::now();
    let mut should_quit = false;

    // Initial tick to render something
    tick_fn.call(&mut host.store, (0.0,))?;

    loop {
        if should_quit { break; }

        let mut input_val = GridInput::default();
        let mut input_received = false;

        // --- Event Polling ---
        // If tick_rate is 0, we block (wait) for input to save CPU.
        // If tick_rate > 0, we poll with a short timeout to maintain frame rate.
        let poll_timeout = if tick_rate == 0.0 {
            Duration::from_millis(100) // Small timeout to allow check of other conditions if needed
        } else {
            Duration::from_millis(1) // Fast poll
        };

        if event::poll(poll_timeout)? {
            let evt = event::read()?;
            match evt {
                Event::Key(key) => {
                    if key.code == KeyCode::Esc {
                        should_quit = true;
                    }
                    input_val = map_key(key);
                    input_received = true;
                }
                _ => {} // Ignore mouse/resize for MVP
            }
        }

        // --- Ticking Logic ---
        let should_tick = if tick_rate == 0.0 {
            // Tick only if we got input
            input_received
        } else {
            // Tick if enough time passed
            last_tick.elapsed().as_secs_f32() >= (1.0 / tick_rate)
        };

        if should_tick {
             // 1. Update Input in WASM Memory
             let bytes = bytemuck::bytes_of(&input_val);
             host.write_mem(input_ptr, bytes)?;
             
             // 2. Notify Driver of Input Pointer
             set_input_fn.call(&mut host.store, (input_ptr,))?;

             // 3. Call Tick
             // Calculate delta if needed, for now fixed or actual elapsed
             let delta = last_tick.elapsed().as_secs_f32();
             tick_fn.call(&mut host.store, (delta,))?;
             
             last_tick = Instant::now();
        }

        // --- Rendering ---
        // We render every loop iteration to keep UI responsive (e.g. if we add UI outside the grid)
        // Retrieve Grid Info
        let dims = get_dims_fn.call(&mut host.store, ())?;
        let width = (dims >> 32) as i32;
        let height = (dims & 0xFFFFFFFF) as i32;
        let grid_ptr = get_ptr_fn.call(&mut host.store, ())?;

        // Read Grid Data
        let grid_byte_len = width * height * std::mem::size_of::<GridCell>() as i32;
        let grid_data = host.read_mem(grid_ptr, grid_byte_len)?;
        let cells: &[GridCell] = bytemuck::cast_slice(&grid_data);

        terminal.draw(|f| {
            let area = f.area();
            let buf = f.buffer_mut();
            
            // Render the Grid
            for y in 0..height {
                for x in 0..width {
                    // Bounds check against screen size
                    if (x as u16) < area.width && (y as u16) < area.height {
                        let idx = (y * width + x) as usize;
                        if idx < cells.len() {
                            let cell = &cells[idx];
                            // Only draw if char is valid
                            if let Some(ch) = std::char::from_u32(cell.character) {
                                // Basic Color Mapping (ANSI 256)
                                let fg = Color::Indexed(cell.fg_color);
                                let bg = Color::Indexed(cell.bg_color);
                                
                                buf.get_mut(x as u16, y as u16)
                                   .set_char(ch)
                                   .set_fg(fg)
                                   .set_bg(bg);
                            }
                        }
                    }
                }
            }
        })?;
    }

    // --- Cleanup ---
    disable_raw_mode()?;
    execute!(std::io::stdout(), LeaveAlternateScreen)?;
    println!("👋 GridEmbedder Exited.");
    Ok(())
}