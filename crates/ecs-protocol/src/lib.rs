#![no_std]

// Capabilities (Embedder checks these)
pub const CAPABILITY_TUI: &str = "capability_tui_renderer";

// --- COMPONENT DATA LAYOUTS ---

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TileStatus {
    Hidden = 0,
    Revealed = 1,
    Flagged = 2,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Tile {
    pub is_mine: bool,
    pub adjacent_mines: u8,
    pub status: TileStatus,
}

// Singleton for the player's cursor
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Cursor {
    pub x: i32,
    pub y: i32,
}

// Handshake
#[repr(C)]
pub struct StandardIds {
    pub position_id: i32,
    pub tile_id: i32,
    pub cursor_id: i32,
}
