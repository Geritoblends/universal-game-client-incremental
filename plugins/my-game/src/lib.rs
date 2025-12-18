use ecs_client::{export_grid, register_plugin, App, Res, ResMut, Resource, Schedule};

// shared-structs/src/lib.rs
// (Or put this at the top of my-game/src/lib.rs)

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub is_mine: bool,
    pub is_revealed: bool,
    pub is_flagged: bool,
    pub neighbors: u8,
}

// Default state for a cell
impl Default for Cell {
    fn default() -> Self {
        Self {
            is_mine: false,
            is_revealed: false,
            is_flagged: false,
            neighbors: 0,
        }
    }
}

pub const MAX_WIDTH: usize = 32;
pub const MAX_HEIGHT: usize = 16;
pub const MAX_CELLS: usize = MAX_WIDTH * MAX_HEIGHT;

#[repr(C)]
pub struct GameGrid {
    pub width: i32,
    pub height: i32,
    pub cursor_x: i32,
    pub cursor_y: i32,
    pub game_over: bool,
    pub cells: [Cell; MAX_CELLS],
}

// --- IDs ---
// Host and Guest must agree on these manual IDs
pub const GRID_RES_ID: i32 = 100;
pub const INPUT_RES_ID: i32 = 101;

#[repr(C)]
pub struct InputState {
    pub dx: i32, // -1, 0, 1 (Movement)
    pub dy: i32,
    pub reveal: bool, // Spacebar pressed?
    pub flag: bool,   // 'F' pressed?
}
// use shared_structs::{GameGrid, InputState, Cell, GRID_RES_ID, INPUT_RES_ID, MAX_WIDTH, MAX_HEIGHT};
// (Pasting the structs here for a self-contained example if needed, but assuming import)

// --- 1. RESOURCE WIRING ---

// Tell ECS that GameGrid lives at ID 100
impl Resource for GameGrid {
    fn resource_id() -> i32 {
        GRID_RES_ID
    }
}

// Tell ECS that InputState lives at ID 101
impl Resource for InputState {
    fn resource_id() -> i32 {
        INPUT_RES_ID
    }
}

// Create the "get_grid_ptr" export for the Host
export_grid!(GameGrid);

// --- 2. SYSTEMS ---

fn setup_game() {
    let mut grid = ResMut::<GameGrid>::get();

    // 1. Initialize Dimensions
    grid.width = 16;
    grid.height = 10;
    grid.cursor_x = 0;
    grid.cursor_y = 0;
    grid.game_over = false;

    // 2. Clear Board
    for i in 0..MAX_CELLS {
        grid.cells[i] = Cell::default();
    }

    // 3. Place Mines (Pseudo-random)
    // Since Wasm has no system time, we use a simple Linear Congruential Generator
    let mut seed = 12345;
    let mut mines_placed = 0;
    let target_mines = 20;

    while mines_placed < target_mines {
        seed = (seed * 1103515245 + 12345) & 0x7FFFFFFF;
        let idx = (seed as usize) % (grid.width * grid.height) as usize;

        if !grid.cells[idx].is_mine {
            grid.cells[idx].is_mine = true;
            mines_placed += 1;
        }
    }

    // 4. Calculate Neighbors
    // We can't query "neighbors" easily in a 1D array, so we do coordinate math.
    let w = grid.width;
    let h = grid.height;

    for y in 0..h {
        for x in 0..w {
            let idx = (y * 32 + x) as usize; // Stride is ALWAYS 32 (MAX_WIDTH)

            if grid.cells[idx].is_mine {
                continue;
            }

            let mut count = 0;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }

                    let nx = x + dx;
                    let ny = y + dy;

                    if nx >= 0 && nx < w && ny >= 0 && ny < h {
                        let n_idx = (ny * 32 + nx) as usize;
                        if grid.cells[n_idx].is_mine {
                            count += 1;
                        }
                    }
                }
            }
            grid.cells[idx].neighbors = count;
        }
    }
}

fn game_logic() {
    let mut grid = ResMut::<GameGrid>::get();
    let input = Res::<InputState>::get();

    if grid.game_over {
        return;
    }

    // 1. Handle Movement
    if input.dx != 0 || input.dy != 0 {
        grid.cursor_x = (grid.cursor_x + input.dx).clamp(0, grid.width - 1);
        grid.cursor_y = (grid.cursor_y + input.dy).clamp(0, grid.height - 1);
    }

    let cursor_idx = (grid.cursor_y * 32 + grid.cursor_x) as usize;

    // 2. Handle Flagging
    if input.flag {
        let cell = &mut grid.cells[cursor_idx];
        if !cell.is_revealed {
            cell.is_flagged = !cell.is_flagged;
        }
    }

    // 3. Handle Reveal
    if input.reveal {
        let cell = &mut grid.cells[cursor_idx];
        if !cell.is_flagged && !cell.is_revealed {
            if cell.is_mine {
                cell.is_revealed = true;
                grid.game_over = true; // BOOM
            } else {
                flood_fill_reveal(&mut grid, grid.cursor_x, grid.cursor_y);
            }
        }
    }
}

// Recursive Flood Fill (Stack-safeish version)
fn flood_fill_reveal(grid: &mut GameGrid, x: i32, y: i32) {
    let idx = (y * 32 + x) as usize;
    let cell = &mut grid.cells[idx];

    if cell.is_revealed || cell.is_flagged {
        return;
    }

    cell.is_revealed = true;

    // If it's a number (neighbors > 0), we stop.
    // If it's blank (neighbors == 0), we recurse.
    if cell.neighbors > 0 {
        return;
    }

    for dy in -1..=1 {
        for dx in -1..=1 {
            let nx = x + dx;
            let ny = y + dy;
            if nx >= 0 && nx < grid.width && ny >= 0 && ny < grid.height {
                flood_fill_reveal(grid, nx, ny);
            }
        }
    }
}

// --- 3. ENTRY POINT ---

fn setup(app: &mut App) {
    app.add_systems(Schedule::Startup, setup_game);
    app.add_systems(Schedule::Update, game_logic);
}

register_plugin!(setup);
