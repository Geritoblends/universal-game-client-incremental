use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{prelude::*, widgets::*};
use std::fs;
use std::io::stdout;
use std::time::Duration;

// --- MODULES ---
pub mod allocator;
pub mod host;
pub mod host_calls;

// Use your existing BlindHost structure
use host::host_object::{BlindHost, BlindHostConfig};

// --- SHARED DATA CONTRACT ---
// These must match the memory layout of the Wasm guest exactly.

const GRID_RES_ID: i32 = 100;
const INPUT_RES_ID: i32 = 101;
const MAX_WIDTH: usize = 32;
const MAX_HEIGHT: usize = 16;
const MAX_CELLS: usize = MAX_WIDTH * MAX_HEIGHT;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct Cell {
    pub is_mine: bool,
    pub is_revealed: bool,
    pub is_flagged: bool,
    pub neighbors: u8,
}

#[repr(C)]
struct GameGrid {
    pub width: i32,
    pub height: i32,
    pub cursor_x: i32,
    pub cursor_y: i32,
    pub game_over: bool,
    pub cells: [Cell; MAX_CELLS],
}

#[repr(C)]
struct InputState {
    pub dx: i32,
    pub dy: i32,
    pub reveal: bool,
    pub flag: bool,
}

// --- APP STATE ---

#[derive(Clone, Copy, PartialEq)]
enum AppScreen {
    Menu,
    Game,
    Exiting,
}

struct AppState {
    screen: AppScreen,
    menu_index: usize,
    // We hold the host here so we can tick it
    host: Option<BlindHost>,
}

fn main() -> Result<()> {
    // 1. Terminal Setup
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app_state = AppState {
        screen: AppScreen::Menu,
        menu_index: 0,
        host: None,
    };

    // 2. Main Loop
    loop {
        // A. Draw
        terminal.draw(|frame| {
            let area = frame.area();
            match app_state.screen {
                AppScreen::Menu => render_menu(frame, area, &app_state),
                AppScreen::Game => {
                    if let Some(host) = &mut app_state.host {
                        // In a real app, you might want to separate "Update" from "Render".
                        // Here, we just peek at the memory to render.
                        if let Ok(grid) = read_grid_from_host(host) {
                            render_game(frame, area, &grid);
                        } else {
                            // Fallback if read fails
                            frame.render_widget(Paragraph::new("Error reading Grid"), area);
                        }
                    }
                }
                _ => {}
            }
        })?;

        // B. Input
        if event::poll(Duration::from_millis(30))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match app_state.screen {
                        AppScreen::Menu => handle_menu_input(&mut app_state, key.code)?,
                        AppScreen::Game => handle_game_input(&mut app_state, key.code)?,
                        _ => {}
                    }
                }
            }
        }

        if app_state.screen == AppScreen::Exiting {
            break;
        }
    }

    // 3. Cleanup
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

// --- LOGIC: HOST INTEGRATION ---

fn init_game_host() -> Result<BlindHost> {
    // 1. Create BlindHost
    let config = BlindHostConfig::default();
    let mut host = BlindHost::new(config, |_, _| Ok(()))?;

    // 2. Load Kernel (ecs-core)
    // This provides the 'sys_resource' and memory management infrastructure
    let core_wasm = fs::read("../target/wasm32-unknown-unknown/release/ecs_core.wasm")
        .context("Failed to read ecs_core.wasm")?;
    host.load_plugin("ecs-core", &core_wasm)?;

    // 3. Load Game (my-game)
    let game_wasm = fs::read("../target/wasm32-unknown-unknown/release/my_game.wasm")
        .context("Failed to read my_game.wasm")?;
    host.load_plugin("my-game", &game_wasm)?;

    // 4. Initialize Game Plugin
    // The register_plugin! macro usually creates 'plugin_init'
    let init_func = host.get_func("my-game", "plugin_init")?;
    init_func
        .typed::<(), ()>(&mut host.store)?
        .call(&mut host.store, ())?;

    Ok(host)
}

fn tick_game(host: &mut BlindHost, input: InputState) -> Result<()> {
    // 1. Get pointer to Input Resource from ECS Kernel
    // We call ecs-core.sys_resource(ID, SIZE)
    let sys_res = host.get_func("ecs-core", "sys_resource")?;
    let ptr_i32 = sys_res.typed::<(i32, i32), i32>(&mut host.store)?.call(
        &mut host.store,
        (INPUT_RES_ID, std::mem::size_of::<InputState>() as i32),
    )?;

    // 2. Write Input Data
    if ptr_i32 != 0 {
        let bytes = unsafe {
            std::slice::from_raw_parts(
                &input as *const _ as *const u8,
                std::mem::size_of::<InputState>(),
            )
        };
        host.write_mem(ptr_i32, bytes)?;
    }

    // 3. Update Game Logic
    let update_func = host.get_func("my-game", "plugin_update")?;
    update_func
        .typed::<(), ()>(&mut host.store)?
        .call(&mut host.store, ())?;

    Ok(())
}

fn read_grid_from_host(host: &mut BlindHost) -> Result<GameGrid> {
    // 1. Get pointer to Grid
    // The plugin exports a helper 'get_grid_ptr' (via export_grid! macro)
    let get_ptr = host.get_func("my-game", "get_grid_ptr")?;
    let ptr = get_ptr
        .typed::<(), i32>(&mut host.store)?
        .call(&mut host.store, ())?;

    // 2. Read Bytes
    let bytes = host.read_mem(ptr, std::mem::size_of::<GameGrid>() as i32)?;

    // 3. Cast to Struct (Safety: BlindHost copy ensures alignment is handled by Vec usually)
    // To be strictly safe against alignment issues, we should use `ptr::read_unaligned`,
    // but for this demo, direct pointer casting of the buffer works if struct is simple.
    let grid = unsafe { std::ptr::read(bytes.as_ptr() as *const GameGrid) };
    Ok(grid)
}

// --- UI HANDLERS ---

fn handle_menu_input(state: &mut AppState, key: KeyCode) -> Result<()> {
    match key {
        KeyCode::Up | KeyCode::Char('k') => {
            if state.menu_index > 0 {
                state.menu_index -= 1
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if state.menu_index < 1 {
                state.menu_index += 1
            }
        }
        KeyCode::Enter | KeyCode::Char(' ') => match state.menu_index {
            0 => {
                // Load and Switch
                state.host = Some(init_game_host()?);
                state.screen = AppScreen::Game;
            }
            1 => state.screen = AppScreen::Exiting,
            _ => {}
        },
        KeyCode::Char('q') => state.screen = AppScreen::Exiting,
        _ => {}
    }
    Ok(())
}

fn handle_game_input(state: &mut AppState, key: KeyCode) -> Result<()> {
    if let Some(host) = &mut state.host {
        let mut input = InputState {
            dx: 0,
            dy: 0,
            reveal: false,
            flag: false,
        };

        match key {
            KeyCode::Esc | KeyCode::Char('q') => {
                state.screen = AppScreen::Menu;
                state.host = None; // Unload
                return Ok(());
            }
            KeyCode::Char('r') => {
                // Reload
                state.host = Some(init_game_host()?);
                return Ok(());
            }
            KeyCode::Up | KeyCode::Char('k') => input.dy = -1,
            KeyCode::Down | KeyCode::Char('j') => input.dy = 1,
            KeyCode::Left | KeyCode::Char('h') => input.dx = -1,
            KeyCode::Right | KeyCode::Char('l') => input.dx = 1,
            KeyCode::Char(' ') => input.reveal = true,
            KeyCode::Char('f') => input.flag = true,
            _ => {}
        }

        tick_game(host, input)?;
    }
    Ok(())
}

// --- RENDERING ---

fn render_menu(frame: &mut Frame, area: Rect, state: &AppState) {
    let layout = Layout::vertical([
        Constraint::Percentage(40),
        Constraint::Length(10),
        Constraint::Min(0),
    ])
    .split(area);

    let title = Paragraph::new("BLIND HOST LAUNCHER")
        .style(Style::default().fg(Color::Cyan).bold())
        .alignment(Alignment::Center);
    frame.render_widget(title, layout[0]);

    let items = vec!["Start Minesweeper", "Quit"];
    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(i, &t)| {
            let style = if i == state.menu_index {
                Style::default().bg(Color::Cyan).fg(Color::Black)
            } else {
                Style::default().fg(Color::Gray)
            };
            ListItem::new(format!(" {} ", t)).style(style)
        })
        .collect();

    frame.render_widget(
        List::new(list_items).block(Block::default().borders(Borders::NONE)),
        Layout::horizontal([
            Constraint::Percentage(40),
            Constraint::Percentage(20),
            Constraint::Percentage(40),
        ])
        .split(layout[1])[1],
    );
}

fn render_game(frame: &mut Frame, area: Rect, grid: &GameGrid) {
    let layout = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(area);

    // Status
    let status_text = if grid.game_over {
        "💥 GAME OVER 💥 (Press 'R' to Restart)".red().bold()
    } else {
        Span::raw(format!("Cursor: {},{}", grid.cursor_x, grid.cursor_y))
    };
    frame.render_widget(
        Paragraph::new(status_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Minesweeper "),
            )
            .alignment(Alignment::Center),
        layout[0],
    );

    // Board
    let block = Block::default().borders(Borders::ALL);
    let inner = block.inner(layout[1]);
    frame.render_widget(block, layout[1]);

    let offset_x = inner.x + (inner.width.saturating_sub((grid.width * 3) as u16) / 2);
    let offset_y = inner.y + (inner.height.saturating_sub(grid.height as u16) / 2);

    for y in 0..grid.height {
        for x in 0..grid.width {
            if x < 0 || y < 0 || x >= 32 || y >= 16 {
                continue;
            }

            let idx = (y * 32 + x) as usize;
            let cell = &grid.cells[idx];

            let draw_x = offset_x + (x as u16 * 3);
            let draw_y = offset_y + (y as u16);

            if draw_x >= inner.right() || draw_y >= inner.bottom() {
                continue;
            }

            let is_cursor = x == grid.cursor_x && y == grid.cursor_y;

            let (sym, mut style) = match (cell.is_revealed, cell.is_flagged) {
                (false, true) => (" 🚩", Style::default().fg(Color::Red)),
                (false, false) => (" · ", Style::default().fg(Color::DarkGray)),
                (true, _) => {
                    if cell.is_mine {
                        (" 💣", Style::default().bg(Color::Red))
                    } else {
                        match cell.neighbors {
                            0 => ("   ", Style::default()),
                            1 => (" 1 ", Style::default().fg(Color::Blue)),
                            2 => (" 2 ", Style::default().fg(Color::Green)),
                            3 => (" 3 ", Style::default().fg(Color::Red)),
                            _ => (" ? ", Style::default().fg(Color::Yellow)),
                        }
                    }
                }
            };

            if is_cursor {
                style = style.bg(Color::White).fg(Color::Black);
            }

            frame.render_widget(
                Paragraph::new(sym).style(style),
                Rect::new(draw_x, draw_y, 3, 1),
            );
        }
    }

    // Footer
    frame.render_widget(
        Paragraph::new("Arrows: Move | Space: Reveal | F: Flag | R: Restart | Q: Quit")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center),
        layout[2],
    );
}
