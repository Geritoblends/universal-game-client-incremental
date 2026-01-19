use grid_protocol::{
    GridCell, GridInput, INPUT_KEY, 
    KEY_UP, KEY_DOWN, KEY_LEFT, KEY_RIGHT
};
use std::sync::Mutex;
use once_cell::sync::Lazy;

#[global_allocator]
static ALLOC: tasksapp_allocator::HostAllocator = tasksapp_allocator::HostAllocator;

#[derive(Clone, Copy, PartialEq)]
enum Direction {
    Up,
    Down,
    Left,
    Right,
}

struct SnakeState {
    width: i32,
    height: i32,
    cells: Vec<GridCell>,
    snake: Vec<(i32, i32)>,
    direction: Direction,
    next_direction: Direction,
    food: (i32, i32),
    rng_state: u32,
    score: i32,
    game_over: bool,
    accumulated_time: f32,
    move_interval: f32,
    last_input: GridInput,
}

impl SnakeState {
    fn new(width: i32, height: i32) -> Self {
        let mut cells = vec![GridCell::default(); (width * height) as usize];
        let snake = vec![(width / 2, height / 2), (width / 2 - 1, height / 2), (width / 2 - 2, height / 2)];
        let mut state = SnakeState {
            width,
            height,
            cells,
            snake,
            direction: Direction::Right,
            next_direction: Direction::Right,
            food: (0, 0),
            rng_state: 42, // Seed
            score: 0,
            game_over: false,
            accumulated_time: 0.0,
            move_interval: 0.15, // Move every 150ms
            last_input: GridInput::default(),
        };
        state.spawn_food();
        state
    }

    fn spawn_food(&mut self) {
        loop {
            // Xorshift32
            let mut x = self.rng_state;
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            self.rng_state = x;

            let food_x = (x % self.width as u32) as i32;
            
            let mut y = self.rng_state;
            y ^= y << 13;
            y ^= y >> 17;
            y ^= y << 5;
            self.rng_state = y;
            let food_y = (y % self.height as u32) as i32;

            if !self.snake.contains(&(food_x, food_y)) {
                self.food = (food_x, food_y);
                break;
            }
        }
    }

    fn update(&mut self, delta: f32) {
        if self.game_over {
            return;
        }

        self.accumulated_time += delta;
        if self.accumulated_time < self.move_interval {
            return;
        }
        self.accumulated_time -= self.move_interval;

        self.direction = self.next_direction;
        let head = self.snake[0];
        let mut new_head = match self.direction {
            Direction::Up => (head.0, head.1 - 1),
            Direction::Down => (head.0, head.1 + 1),
            Direction::Left => (head.0 - 1, head.1),
            Direction::Right => (head.0 + 1, head.1),
        };

        // Wrap around or Game Over? Let's do Game Over for classic feel
        if new_head.0 < 0 || new_head.0 >= self.width || new_head.1 < 0 || new_head.1 >= self.height {
            self.game_over = true;
            return;
        }

        if self.snake.contains(&new_head) {
            self.game_over = true;
            return;
        }

        self.snake.insert(0, new_head);

        if new_head == self.food {
            self.score += 10;
            self.spawn_food();
            // Speed up slightly
            if self.move_interval > 0.05 {
                self.move_interval -= 0.002;
            }
        } else {
            self.snake.pop();
        }
    }

    fn draw(&mut self) {
        // Clear
        for cell in self.cells.iter_mut() {
            cell.character = ' ' as u32;
            cell.bg_color = 0;
            cell.fg_color = 15;
        }

        // Render Debug info (Input) at top left
        if self.last_input.input_type == INPUT_KEY {
            // Just show the key code as a char if possible
            let msg = if self.last_input.key_code < 0x110000 {
                 if let Some(c) = char::from_u32(self.last_input.key_code) {
                     format!("Input: {}", c)
                 } else {
                    format!("Input Code: {}", self.last_input.key_code)
                 }
            } else {
                format!("Input Special: {:X}", self.last_input.key_code)
            };
            
            for (i, char_val) in msg.chars().enumerate() {
                 if i < self.width as usize {
                     self.cells[i].character = char_val as u32;
                     self.cells[i].fg_color = 14; // Cyan
                 }
            }
        }

        if self.game_over {
            let msg = format!(" GAME OVER! Score: {} ", self.score);
            let x_start = (self.width - msg.len() as i32) / 2;
            let y = self.height / 2;
            for (i, c) in msg.chars().enumerate() {
                let x = x_start + i as i32;
                if x >= 0 && x < self.width {
                    let idx = (y * self.width + x) as usize;
                    self.cells[idx].character = c as u32;
                    self.cells[idx].fg_color = 196; // Red
                }
            }
            return;
        }

        // Draw Food
        let food_idx = (self.food.1 * self.width + self.food.0) as usize;
        self.cells[food_idx].character = '●' as u32;
        self.cells[food_idx].fg_color = 160; // Reddish

        // Draw Snake
        for (i, &(sx, sy)) in self.snake.iter().enumerate() {
            let idx = (sy * self.width + sx) as usize;
            if i == 0 {
                self.cells[idx].character = '◆' as u32; // Head
                self.cells[idx].fg_color = 46; // Bright Green
            } else {
                self.cells[idx].character = '■' as u32; // Body
                self.cells[idx].fg_color = 34; // Green
            }
        }

        // Draw Score
        let score_msg = format!("Score: {}", self.score);
        for (i, c) in score_msg.chars().enumerate() {
            if (i as i32) < self.width {
                self.cells[i].character = c as u32;
                self.cells[i].fg_color = 226; // Yellow
            }
        }
    }
}

static STATE: Lazy<Mutex<SnakeState>> = Lazy::new(|| {
    Mutex::new(SnakeState::new(80, 24))
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
pub extern "C" fn set_tickrate(_rate: f32) {
    // We handle our own internal tickrate for movement
}

#[no_mangle]
pub extern "C" fn set_input(ptr: i32) {
    let mut state = STATE.lock().unwrap();
    let input_ptr = ptr as *const GridInput;
    let input = unsafe { *input_ptr };
    state.last_input = input;

    if input.input_type == INPUT_KEY {
        match input.key_code {
            KEY_UP if state.direction != Direction::Down => state.next_direction = Direction::Up,
            KEY_DOWN if state.direction != Direction::Up => state.next_direction = Direction::Down,
            KEY_LEFT if state.direction != Direction::Right => state.next_direction = Direction::Left,
            KEY_RIGHT if state.direction != Direction::Left => state.next_direction = Direction::Right,
            _ => {}
        }
    }
}

#[no_mangle]
pub extern "C" fn tick(delta: f32) {
    let mut state = STATE.lock().unwrap();
    state.update(delta);
    state.draw();
}
