use tasksapp_ecs_client::*;

// --- 1. Define Components ---

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}

// We must implement the Component trait from your client
impl Component for Position {
    const ID: i32 = 1;
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Velocity {
    pub dx: f32,
    pub dy: f32,
}

impl Component for Velocity {
    const ID: i32 = 2;
}

// --- 2. Define System ---

// The function signature matches 'extern "C" fn(i32)'
extern "C" fn sys_movement(_: i32) {
    // Create the query. Internally, this calls ffi::query_archetypes
    let query = Query::<(&mut Position, &Velocity)>::new();

    query.for_each(|(pos, vel)| {
        pos.x += vel.dx;
        pos.y += vel.dy;

        // Since ecs_client sets up the GlobalAllocator, we can use format!
        let msg = format!("🏃 [GAME] Entity moved to: ({:.2}, {:.2})", pos.x, pos.y);
        print(&msg);
    });
}

// --- 3. Initialize ---

#[no_mangle]
pub extern "C" fn init() {
    print("🎮 [GAME] Initializing...");

    // A. Register Components
    register_component::<Position>();
    register_component::<Velocity>();

    // B. Register System
    // We pass the function pointer directly.
    // Note: The generic <(...)> is just for type safety/metadata in your API.
    register_system::<(&mut Position, &Velocity)>("MovementSystem", sys_movement);

    // C. Spawn Entity
    let e = spawn_entity();

    // D. Add Components
    add_component(e, &Position { x: 0.0, y: 0.0 });
    add_component(e, &Velocity { dx: 1.5, dy: 0.5 });

    print("✨ [GAME] Entity spawned.");
}
