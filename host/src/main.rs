use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{prelude::*, widgets::*};
use std::fs;
use std::io::stdout;

// --- MODULES ---
pub mod allocator;
pub mod host;
pub mod host_calls;

use ecs_protocol::{Cursor, Position, StandardIds, Tile, TileStatus};
use host::host_object::{BlindHost, BlindHostConfig};

// --- HELPER FUNCTIONS ---

fn get_column_info(host: &mut BlindHost, table_idx: i32, comp_id: i32) -> Result<(i32, i32)> {
    let get_col_fn = host
        .get_func("ecs_core", "get_table_column")?
        .typed::<(i32, i32), i64>(&mut host.store)?;

    let packed = get_col_fn.call(&mut host.store, (table_idx, comp_id))?;
    if packed == 0 {
        return Ok((0, 0));
    }

    let len = (packed >> 32) as i32;
    let ptr = (packed & 0xFFFFFFFF) as i32;
    Ok((ptr, len))
}

fn fetch_game_data(
    host: &mut BlindHost,
    tile_id: i32,
    pos_id: i32,
    results: &mut Vec<(Position, Tile)>,
) -> Result<()> {
    results.clear();

    // 1. Alloc & Write Query IDs
    let query_ids = [pos_id, tile_id];
    let query_size = 8;
    let query_ptr = guest_alloc(host, query_size)?;

    let mut query_bytes = Vec::new();
    for id in query_ids {
        query_bytes.extend_from_slice(&id.to_le_bytes());
    }
    host.write_mem(query_ptr, &query_bytes)?;

    // 2. Alloc Output Buffer (for table IDs)
    let max_tables = 10;
    let out_ptr = guest_alloc(host, max_tables * 4)?;

    // 3. Run Query
    let query_fn = host
        .get_func("ecs_core", "query_archetypes")?
        .typed::<(i32, i32, i32, i32), i32>(&mut host.store)?;

    let table_count = query_fn.call(&mut host.store, (query_ptr, 2, out_ptr, max_tables))?;

    // 4. Read Results
    if table_count > 0 {
        let table_bytes = host.read_mem(out_ptr, table_count * 4)?;
        let table_indices = unsafe {
            std::slice::from_raw_parts(table_bytes.as_ptr() as *const i32, table_count as usize)
        };

        for &table_idx in table_indices {
            let (pos_ptr, pos_len) = get_column_info(host, table_idx, pos_id)?;
            let (tile_ptr, tile_len) = get_column_info(host, table_idx, tile_id)?;

            if pos_len > 0 && pos_len == tile_len {
                let p_bytes = host.read_mem(pos_ptr, pos_len * 8)?;
                let t_bytes = host.read_mem(tile_ptr, tile_len * 2)?;

                let positions: &[Position] = unsafe {
                    std::slice::from_raw_parts(
                        p_bytes.as_ptr() as *const Position,
                        pos_len as usize,
                    )
                };
                let tiles: &[Tile] = unsafe {
                    std::slice::from_raw_parts(t_bytes.as_ptr() as *const Tile, tile_len as usize)
                };

                for (p, t) in positions.iter().zip(tiles.iter()) {
                    results.push((*p, *t));
                }
            }
        }
    }

    // 5. Cleanup
    guest_dealloc(host, query_ptr, query_size);
    guest_dealloc(host, out_ptr, max_tables * 4);

    Ok(())
}

fn fetch_cursor(host: &mut BlindHost, cursor_id: i32) -> Result<Cursor> {
    let mut cursor = Cursor { x: 0, y: 0 };

    let query_ptr = guest_alloc(host, 4)?;
    host.write_mem(query_ptr, &cursor_id.to_le_bytes())?;

    let out_ptr = guest_alloc(host, 4)?;
    let query_fn = host
        .get_func("ecs_core", "query_archetypes")?
        .typed::<(i32, i32, i32, i32), i32>(&mut host.store)?;

    let count = query_fn.call(&mut host.store, (query_ptr, 1, out_ptr, 1))?;

    if count > 0 {
        let table_bytes = host.read_mem(out_ptr, 4)?;
        let table_idx = i32::from_le_bytes(table_bytes[0..4].try_into()?);

        let (ptr, len) = get_column_info(host, table_idx, cursor_id)?;
        if len > 0 {
            let bytes = host.read_mem(ptr, 8)?;
            let c: Cursor = unsafe { std::ptr::read_unaligned(bytes.as_ptr() as *const _) };
            cursor = c;
        }
    }

    guest_dealloc(host, query_ptr, 4);
    guest_dealloc(host, out_ptr, 4);

    Ok(cursor)
}

fn main() -> Result<()> {
    // 1. Setup Terminal
    stdout().execute(EnterAlternateScreen)?;
    enable_raw_mode()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    // 2. Setup Host
    let config = BlindHostConfig::default();
    let mut host = BlindHost::new(config, |_, _| Ok(()))?;

    // 3. Load Plugins
    let core_path = "target/wasm32-unknown-unknown/release/ecs_core.wasm";
    let game_path = "target/wasm32-unknown-unknown/release/my_game.wasm";

    let load_wasm = |path: &str| -> Result<Vec<u8>> {
        fs::read(path).with_context(|| format!("Could not find WASM at '{}'", path))
    };

    host.load_plugin("ecs_core", &load_wasm(core_path)?)?;
    host.load_plugin("minesweeper", &load_wasm(game_path)?)?;

    // 4. Handshake (Get IDs)
    let get_ids_fn = host
        .get_func("ecs_core", "get_standard_ids")?
        .typed::<(), i64>(&mut host.store)?;
    let ptr = get_ids_fn.call(&mut host.store, ())?;

    let bytes = host.read_mem(ptr as i32, 12)?;
    let ids: StandardIds = unsafe { std::ptr::read_unaligned(bytes.as_ptr() as *const _) };

    // 5. Get Game Functions (Input & Tick)
    let input_fn = host
        .get_func("minesweeper", "on_input")?
        .typed::<i32, ()>(&mut host.store)?;

    // CRITICAL FIX: We need to "tick" the game engine so systems run!
    // Try to get "tick" or "update". If your game exports "update", change this string.
    let tick_fn = host
        .get_func("minesweeper", "tick")
        .or_else(|_| host.get_func("minesweeper", "update"))
        .context("Could not find 'tick' or 'update' function in minesweeper plugin")?
        .typed::<(), ()>(&mut host.store)?;

    let mut tiles: Vec<(Position, Tile)> = Vec::with_capacity(100);

    // 6. Main Loop
    loop {
        // --- GAME LOGIC ---
        // Run the game systems (Spawn entities, handle logic, etc.)
        tick_fn.call(&mut host.store, ())?;

        // --- RENDER ---
        terminal.draw(|frame| {
            let area = frame.area();
            let mut cursor = Cursor { x: 0, y: 0 };

            // Fetch data *after* the tick so we see the results of this frame
            let _ = fetch_game_data(&mut host, ids.tile_id, ids.position_id, &mut tiles);
            if let Ok(c) = fetch_cursor(&mut host, ids.cursor_id) {
                cursor = c;
            }

            let block = Block::default()
                .borders(Borders::ALL)
                .title(format!(" Minesweeper (Entities: {}) ", tiles.len()));
            let inner_area = block.inner(area);
            frame.render_widget(block, area);

            for (pos, tile) in &tiles {
                // Ensure coordinates don't overflow the UI area
                let x = inner_area.x + (pos.x as u16 * 3);
                let y = inner_area.y + (pos.y as u16);

                if x >= inner_area.right() || y >= inner_area.bottom() {
                    continue;
                }

                let symbol = match tile.status {
                    TileStatus::Hidden => " · ",
                    TileStatus::Flagged => " 🚩",
                    TileStatus::Revealed => {
                        if tile.is_mine {
                            " 💣"
                        } else {
                            "   "
                        }
                    }
                };

                let style = if pos.x == cursor.x && pos.y == cursor.y {
                    Style::default().bg(Color::Blue).fg(Color::White)
                } else if tile.status == TileStatus::Revealed && tile.is_mine {
                    Style::default().bg(Color::Red)
                } else {
                    Style::default()
                };

                frame.render_widget(Paragraph::new(symbol).style(style), Rect::new(x, y, 3, 1));
            }
        })?;

        // --- INPUT ---
        if event::poll(std::time::Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('q') => break,
                        KeyCode::Char('c')
                            if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                        {
                            break
                        }

                        // Vim Motion Support added here!
                        KeyCode::Up | KeyCode::Char('k') => input_fn.call(&mut host.store, 0)?,
                        KeyCode::Down | KeyCode::Char('j') => input_fn.call(&mut host.store, 1)?,
                        KeyCode::Left | KeyCode::Char('h') => input_fn.call(&mut host.store, 2)?,
                        KeyCode::Right | KeyCode::Char('l') => input_fn.call(&mut host.store, 3)?,

                        KeyCode::Char(' ') => input_fn.call(&mut host.store, 4)?, // Reveal
                        KeyCode::Char('f') => input_fn.call(&mut host.store, 5)?, // Flag
                        _ => {}
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

// --- ALLOCATOR HELPERS ---

const WASM_PAGE_SIZE: u64 = 65536;
const GROWTH_CHUNK_SIZE: u64 = 80;
const HEAP_START_ADDR: u32 = 32 * 1024 * 1024;

fn guest_alloc(host: &mut BlindHost, size: i32) -> Result<i32> {
    let size = (size as u32 + 7) & !7;

    // 1. Try Alloc
    {
        let mut heap = host.store.data().heap.lock().unwrap();
        if let Some(addr) = heap.alloc(size) {
            return Ok(addr as i32);
        }
    }

    // 2. Grow Memory if needed
    let memory = host.store.data().shared_memory.clone();

    // FIX: Remove &host.store argument
    let current_mem_size = memory.size() * WASM_PAGE_SIZE;

    // We must check if heap is empty to determine start addr
    let heap_is_empty = host
        .store
        .data()
        .heap
        .lock()
        .unwrap()
        .free_blocks
        .is_empty();

    let growth_start_addr = if heap_is_empty && current_mem_size < HEAP_START_ADDR as u64 {
        HEAP_START_ADDR
    } else {
        current_mem_size as u32
    };

    let required_growth = std::cmp::max(
        GROWTH_CHUNK_SIZE,
        (size as u64 + WASM_PAGE_SIZE - 1) / WASM_PAGE_SIZE,
    );

    // FIX: Remove &mut host.store argument
    memory.grow(required_growth)?;

    let new_block_size = (required_growth * WASM_PAGE_SIZE) as u32;

    // 3. Register new memory and alloc again
    {
        let mut heap = host.store.data().heap.lock().unwrap();
        heap.dealloc(growth_start_addr, new_block_size);
        Ok(heap.alloc(size).unwrap_or(0) as i32)
    }
}

fn guest_dealloc(host: &mut BlindHost, ptr: i32, size: i32) {
    if ptr == 0 {
        return;
    }
    let ptr = ptr as u32;
    let size = (size as u32 + 7) & !7;
    host.store.data().heap.lock().unwrap().dealloc(ptr, size);
}
