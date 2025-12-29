use grid_protocol::{GridCell, GridInput, INPUT_KEY};
use std::sync::Mutex;
use once_cell::sync::Lazy;

#[global_allocator]
static ALLOC: tasksapp_allocator::HostAllocator = tasksapp_allocator::HostAllocator;

struct GridState {
    width: i32,
    height: i32,
    cells: Vec<GridCell>,
    tick_rate: f32,
    input: GridInput,
}

static STATE: Lazy<Mutex<GridState>> = Lazy::new(|| {
    let width = 80;
    let height = 24;
    let cells = vec![GridCell::default(); (width * height) as usize];
    Mutex::new(GridState {
        width,
        height,
        cells,
        tick_rate: 0.0,
        input: GridInput::default(),
    })
});

#[no_mangle]
pub extern "C" fn get_grid_dimensions() -> i64 {
    let state = STATE.lock().unwrap();
    let w = state.width as i64;
    let h = state.height as i64;
    (w << 32) | (h & 0xFFFFFFFF)
}

#[no_mangle]
pub extern "C" fn get_grid_ptr() -> i32 {
    let mut state = STATE.lock().unwrap();
    state.cells.as_mut_ptr() as i32
}

#[no_mangle]
pub extern "C" fn set_tickrate(rate: f32) {
    let mut state = STATE.lock().unwrap();
    state.tick_rate = rate;
}

#[no_mangle]
pub extern "C" fn set_input(ptr: i32) {
    let mut state = STATE.lock().unwrap();
    // Safety: The host guarantees this pointer is valid and points to a GridInput
    let input_ptr = ptr as *const GridInput;
    unsafe {
        state.input = *input_ptr;
    }
}

#[no_mangle]
pub extern "C" fn tick(_delta: f32) {
    let mut state = STATE.lock().unwrap();
    
    // Clear grid
    for cell in state.cells.iter_mut() {
        cell.character = ' ' as u32;
        cell.fg_color = 15; // White
        cell.bg_color = 0;  // Black
    }

    // Render Heart
    let cx = state.width / 2;
    let cy = state.height / 2;
    
    // Simple heart shape
    let heart = [
        (0, -1), (-1, -2), (1, -2),
        (-2, -1), (2, -1),
        (-2, 0), (2, 0),
        (-1, 1), (1, 1),
        (0, 2)
    ];

    for (dx, dy) in heart {
         let x = cx + dx;
         let y = cy + dy;
         if x >= 0 && x < state.width && y >= 0 && y < state.height {
             let idx = (y * state.width + x) as usize;
             state.cells[idx].character = '♥' as u32; // Heart symbol
             state.cells[idx].fg_color = 196; // Red
         }
    }
    
    // Render Debug info (Input) at top left
    if state.input.input_type == INPUT_KEY {
        // Just show the key code as a char if possible
        if state.input.key_code < 0x110000 {
             if let Some(c) = char::from_u32(state.input.key_code) {
                 // Write "Input: <char>"
                 let msg = format!("Input: {}", c);
                 for (i, char_val) in msg.chars().enumerate() {
                     if i < state.width as usize {
                         state.cells[i].character = char_val as u32;
                         state.cells[i].fg_color = 14; // Cyan
                     }
                 }
             }
        }
    }
}
