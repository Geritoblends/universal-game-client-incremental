// plugins/my-game/src/lib.rs
use tasksapp_ecs_client::{
    add_component, print, register_component, register_system, spawn_entity, Component, Query,
};

// --- 1. Define Components ---
#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct Position {
    x: f32,
    y: f32,
}
impl Component for Position {
    const ID: i32 = 1;
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct Velocity {
    x: f32,
    y: f32,
}
impl Component for Velocity {
    const ID: i32 = 2;
}

// --- 2. The Logic ---
fn physics_logic((pos, vel): (&mut Position, &Velocity)) {
    pos.x += vel.x;
    pos.y += vel.y;

    // let msg = format!("Entity moved to: {:.1}, {:.1}", pos.x, pos.y);
    // print(&msg);
}

// --- 3. The Wrapper ---
extern "C" fn physics_system_wrapper(_world_ptr: i32) {
    // We ignore _world_ptr for now because we use global imports
    let query = Query::<(&mut Position, &Velocity)>::new();
    query.for_each(|item| {
        physics_logic(item);
    });
}

// --- 4. Init & Setup ---
#[no_mangle]
pub extern "C" fn init() {
    register_component::<Position>();
    register_component::<Velocity>();

    register_system::<(&mut Position, &Velocity)>("Physics", physics_system_wrapper);

    // Spawn 3 entities for testing
    unsafe {
        for _ in 0..3 {
            let entity = spawn_entity();

            let pos = Position { x: 10.0, y: 10.0 };
            let vel = Velocity { x: 1.0, y: 0.0 };

            // FIX: Use the safe API! No raw pointers needed.
            add_component(entity, &pos);
            add_component(entity, &vel);
        }
    }
}
