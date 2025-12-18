// crate: ecs-protocol
use bytemuck::{Pod, Zeroable};

// Component IDs must be consistent between Host and WASM
pub const COMPONENT_POSITION: u32 = 1;
pub const COMPONENT_TILE: u32 = 2;
pub const RESOURCE_CONFIG: u32 = 100;
pub const RESOURCE_STATE: u32 = 101;

// --- COMPONENTS ---
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct Tile {
    pub is_mine: i32, // boolean as i32 for alignment
    pub adj_count: i32,
    pub status: i32, // 0:Hidden, 1:Revealed, 2:Flagged
}

// --- RESOURCES ---
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
pub struct GameConfig {
    pub width: i32,
    pub height: i32,
    pub mine_count: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
pub struct GameState {
    pub is_game_over: i32,
    pub is_victory: i32,
    pub first_move: i32, // acts as bool
}

#[repr(C)] // Crucial for Host-Wasm interoperability
#[derive(Clone, Copy, Debug)]
pub struct Cell {
    pub is_mine: bool,
    pub neighbors: u8,
    pub status: u8,
    pub _padding: u8, // Keep alignment happy
}

pub const MAX_WIDTH: usize = 32;
pub const MAX_HEIGHT: usize = 16;
pub const MAX_CELLS: usize = MAX_WIDTH * MAX_HEIGHT;

#[repr(C)]
pub struct GameGrid {
    pub width: i32,
    pub height: i32,
    pub cells: [Cell; MAX_CELLS],
}

// The "Magic Number" ID for the Grid Resource
pub const GRID_RESOURCE_ID: i32 = 100;
