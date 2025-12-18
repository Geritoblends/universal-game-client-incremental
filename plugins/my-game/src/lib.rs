use ecs_protocol::{self as proto, TileStatus};
use std::ops::{Deref, DerefMut};
use tasksapp_ecs_client::*;

// --- WRAPPERS (The Orphan Rule Fix) ---
// #[repr(transparent)] ensures these have the EXACT same memory layout
// as the inner struct, so the WASM raw pointer casts still work safely.

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Position(pub proto::Position);

impl Component for Position {
    const ID: i32 = 1;
}
impl StandardComponent for Position {
    const KIND: i32 = 1;
}

// Enable accessing .x and .y directly
impl Deref for Position {
    type Target = proto::Position;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for Position {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Tile(pub proto::Tile);

impl Component for Tile {
    const ID: i32 = 2;
}
impl StandardComponent for Tile {
    const KIND: i32 = 2;
}

impl Deref for Tile {
    type Target = proto::Tile;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for Tile {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Cursor(pub proto::Cursor);

impl Component for Cursor {
    const ID: i32 = 3;
}
impl StandardComponent for Cursor {
    const KIND: i32 = 3;
}

impl Deref for Cursor {
    type Target = proto::Cursor;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for Cursor {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// --- GAME LOGIC ---

#[no_mangle]
pub extern "C" fn init() {
    register_std_component::<Position>();
    register_std_component::<Tile>();
    register_std_component::<Cursor>();

    // 1. Spawn Cursor
    let c = spawn_entity();
    add_component(c, &Cursor(proto::Cursor { x: 5, y: 5 }));

    // 2. Generate Grid (10x10)
    let mut rng: u32 = 12345;

    for y in 0..10 {
        for x in 0..10 {
            // All literals here are valid u32
            rng = (rng.wrapping_mul(1103515245).wrapping_add(12345)) % 2147483648;

            // Cast back to logic for the boolean check
            let is_mine = (rng % 10) == 0;

            let e = spawn_entity();
            add_component(e, &Position(proto::Position { x, y }));
            add_component(
                e,
                &Tile(proto::Tile {
                    is_mine,
                    adjacent_mines: 0,
                    status: TileStatus::Hidden,
                }),
            );
        }
    }
}

#[no_mangle]
pub extern "C" fn on_input(code: i32) {
    let query_cursor = Query::<(&mut Cursor)>::new();
    let query_tiles = Query::<(&Position, &mut Tile)>::new();

    // 1. Move Cursor
    let mut cx = 0;
    let mut cy = 0;

    query_cursor.for_each(|cursor| {
        match code {
            0 => cursor.y = (cursor.y - 1).max(0),
            1 => cursor.y = (cursor.y + 1).min(9),
            2 => cursor.x = (cursor.x - 1).max(0),
            3 => cursor.x = (cursor.x + 1).min(9),
            _ => {}
        }
        cx = cursor.x;
        cy = cursor.y;
    });

    // 2. Interact with Tile
    if code == 4 || code == 5 {
        // Reveal or Flag
        query_tiles.for_each(|(pos, tile)| {
            if pos.x == cx && pos.y == cy {
                if code == 5 {
                    // Toggle Flag
                    tile.status = match tile.status {
                        TileStatus::Hidden => TileStatus::Flagged,
                        TileStatus::Flagged => TileStatus::Hidden,
                        _ => tile.status,
                    };
                } else if code == 4 && tile.status == TileStatus::Hidden {
                    // Reveal
                    tile.status = TileStatus::Revealed;
                    if tile.is_mine {
                        print("BOOM! Game Over.");
                    }
                }
            }
        });
    }
}

#[no_mangle]
pub extern "C" fn tick(_dt: f32) {}
