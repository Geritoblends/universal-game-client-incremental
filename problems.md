### Problems we must address

1. The len_bytes / 8 Assumption (Critical) In crates/ecs-client/src/lib.rs:
Rust

// Note: You need a way to know the size of component ID[0] here...
// For now assuming 8 bytes (f32, f32).
let row_count = len_bytes / 8;

If you change Position to be struct Position { x: f32 } (4 bytes), your iteration count will be double what it should be, causing memory corruption when accessing other columns.

    Fix: get_table_column needs to return the row count, or we need to look up the stride dynamically.

2. Component ID Management In my-game/src/lib.rs:
Rust

impl Component for Position { const ID: i32 = 1; }
impl Component for Velocity { const ID: i32 = 2; }

If you add a second plugin (e.g., physics-plugin) and it uses ID 1, it will overwrite Position.

    Fix: We need a dynamic ID registration system where Core assigns IDs at runtime.

* [x] Memory Growth Collision In host/src/host_object.rs:
Rust

next_memory_offset: 1024,
next_stack_offset: 5 * 1024 * 1024, // 5MB

If the first module's data segment grows past 5MB, it will corrupt the second module's stack. Wasm usually guards against stack overflow downwards, but here multiple stacks live in the same linear memory.

4. `run_schedule` calls `Vec::new()`

