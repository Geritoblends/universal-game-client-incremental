use ecs_protocol::{CAPABILITY_TUI, StandardIds};
use once_cell::sync::Lazy;
use rustc_hash::{FxHashMap, FxHashSet};
use std::alloc::{GlobalAlloc, Layout};
use std::panic;
use std::sync::{Arc, Mutex};

// --- HOST IMPORTS ---
#[link(wasm_import_module = "env")]
extern "C" {
    fn host_alloc(size: i32) -> i32;
    fn host_dealloc(ptr: i32, size: i32);
    fn host_print(ptr: i32, len: i32);
    fn host_link_call(m_ptr: *const u8, m_len: usize, f_ptr: *const u8, f_len: usize) -> i32;
}

// --- ALLOCATOR ---
struct HostAllocator;
unsafe impl GlobalAlloc for HostAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        if size == 0 {
            return std::ptr::null_mut();
        }
        let ptr = host_alloc(size as i32);
        ptr as *mut u8
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        host_dealloc(ptr as i32, layout.size() as i32);
    }
}

#[global_allocator]
static ALLOCATOR: HostAllocator = HostAllocator;

static mut POS_ID: i32 = -1;
static mut TILE_ID: i32 = -1; // Was VEL_ID
static mut CURSOR_ID: i32 = -1; // Was SPRITE_ID

// --- UTILS ---
fn print(s: &str) {
    unsafe {
        host_print(s.as_ptr() as i32, s.len() as i32);
    }
}

#[no_mangle]
pub extern "C" fn setup_panic_hook() {
    panic::set_hook(Box::new(|info| {
        let msg = format!("PANIC: {}", info);
        print(&msg);
    }));
}

// --- ECS STRUCTURES (Your Archetype Logic + FxHash) ---

type SystemFn = extern "C" fn(i32);

#[derive(Clone)]
struct SystemMeta {
    name: String,
    func: SystemFn,
    reads: FxHashSet<i32>,
    writes: FxHashSet<i32>,
    dependencies: Vec<String>,
}

struct Stage {
    systems: Vec<SystemFn>,
}

struct Column {
    data: Vec<u8>,
    stride: usize,
}

impl Column {
    fn new(stride: usize, capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity * stride),
            stride,
        }
    }

    unsafe fn push(&mut self, src_ptr: *const u8) {
        let current_len = self.data.len();
        let new_len = current_len + self.stride;
        if new_len > self.data.capacity() {
            self.data.reserve(self.data.capacity().max(new_len));
        }
        self.data.set_len(new_len);
        let dest_ptr = self.data.as_mut_ptr().add(current_len);
        std::ptr::copy_nonoverlapping(src_ptr, dest_ptr, self.stride);
    }

    unsafe fn swap_remove(&mut self, row: usize) -> Vec<u8> {
        let count = self.data.len() / self.stride;
        let last_index = count - 1;
        let mut removed_bytes = vec![0u8; self.stride];

        // Copy removed data
        let src_ptr = self.data.as_ptr().add(row * self.stride);
        std::ptr::copy_nonoverlapping(src_ptr, removed_bytes.as_mut_ptr(), self.stride);

        if row != last_index {
            let last_ptr = self.data.as_ptr().add(last_index * self.stride);
            let hole_ptr = self.data.as_mut_ptr().add(row * self.stride);
            std::ptr::copy_nonoverlapping(last_ptr, hole_ptr, self.stride);
        }
        self.data.set_len(last_index * self.stride);
        removed_bytes
    }
}

struct Table {
    columns: FxHashMap<i32, Column>,
    entities: Vec<i32>,
}

impl Table {
    fn new() -> Self {
        Self {
            columns: FxHashMap::default(),
            entities: Vec::new(),
        }
    }
    fn add_column(&mut self, comp_id: i32, stride: usize) {
        self.columns.insert(comp_id, Column::new(stride, 10));
    }
    fn swap_remove_entity(&mut self, row: usize) -> Option<i32> {
        let last_val = self.entities.pop()?;
        if row < self.entities.len() {
            self.entities[row] = last_val;
            return Some(last_val);
        }
        None
    }
}

pub struct EcsWorld {
    tables: Vec<Table>,
    archetype_map: FxHashMap<Vec<i32>, usize>,
    entity_index: FxHashMap<i32, (usize, usize)>,
    next_entity_id: i32,
    component_strides: FxHashMap<i32, usize>,
    systems: FxHashMap<String, SystemMeta>,
    schedule: Vec<Stage>,
}

// Global Singleton
pub static WORLD: Lazy<Arc<Mutex<EcsWorld>>> = Lazy::new(|| {
    Arc::new(Mutex::new(EcsWorld {
        tables: Vec::new(),
        archetype_map: FxHashMap::default(),
        entity_index: FxHashMap::default(),
        next_entity_id: 0,
        component_strides: FxHashMap::default(),
        systems: FxHashMap::default(),
        schedule: Vec::new(),
    }))
});

// --- API EXPORTS ---

#[no_mangle]
pub extern "C" fn register_component(id: i32, size: i32, _align: i32) {
    let mut world = WORLD.lock().unwrap();
    world.component_strides.insert(id, size as usize);
}

#[no_mangle]
pub extern "C" fn register_system(
    mod_ptr: *const u8,
    mod_len: usize,
    fn_ptr: *const u8,
    fn_len: usize,
) {
    let fn_idx = unsafe { host_link_call(mod_ptr, mod_len, fn_ptr, fn_len) };

    // 2. Read the System Name for our Hash Map
    let name =
        unsafe { String::from_utf8_lossy(std::slice::from_raw_parts(fn_ptr, fn_len)).to_string() };

    // We trust the Host put the correct function at this index in OUR table.
    let func: SystemFn = unsafe { std::mem::transmute(fn_idx as usize) };

    print(&format!(
        "[Core] Linked system '{}' (Table Index: {})",
        name, fn_idx
    ));

    let mut world = WORLD.lock().unwrap();
    world.systems.insert(
        name.clone(),
        SystemMeta {
            name,
            func,
            reads: FxHashSet::default(),
            writes: FxHashSet::default(),
            dependencies: Vec::new(),
        },
    );
}

#[no_mangle]
pub extern "C" fn spawn_entity() -> i32 {
    let mut world = WORLD.lock().unwrap();
    let id = world.next_entity_id;
    world.next_entity_id += 1;
    id
}

#[no_mangle]
pub extern "C" fn add_component(entity_id: i32, comp_id: i32, data_ptr: i32) {
    let mut world = WORLD.lock().unwrap();

    // 1. Current Location
    let current_loc = world.entity_index.get(&entity_id).cloned();

    // 2. Identify Target Archetype
    let mut new_types = Vec::new();
    if let Some((table_idx, _)) = current_loc {
        new_types.extend(world.tables[table_idx].columns.keys().cloned());
    }
    if !new_types.contains(&comp_id) {
        new_types.push(comp_id);
    }
    new_types.sort();

    // 3. Find/Create Target Table
    let target_table_idx = if let Some(&idx) = world.archetype_map.get(&new_types) {
        idx
    } else {
        let mut new_table = Table::new();
        for &cid in &new_types {
            let stride = *world.component_strides.get(&cid).unwrap_or(&0);
            new_table.add_column(cid, stride);
        }
        let idx = world.tables.len();
        world.tables.push(new_table);
        world.archetype_map.insert(new_types.clone(), idx);
        idx
    };

    // 4. Migrate Data
    let mut migrated_data: FxHashMap<i32, Vec<u8>> = FxHashMap::default();
    let mut swapped_entity_update = None;

    if let Some((old_idx, old_row)) = current_loc {
        let old_table = &mut world.tables[old_idx];

        // Remove from old table
        for (&cid, col) in &mut old_table.columns {
            let bytes = unsafe { col.swap_remove(old_row) };
            migrated_data.insert(cid, bytes);
        }

        // Handle swap-remove entity fixup
        if let Some(swapped_entity) = old_table.swap_remove_entity(old_row) {
            swapped_entity_update = Some((swapped_entity, old_idx, old_row));
        }
    }

    if let Some((swapped, idx, row)) = swapped_entity_update {
        world.entity_index.insert(swapped, (idx, row));
    }

    // 5. Add New Component Data
    let stride = *world.component_strides.get(&comp_id).unwrap_or(&0);
    let new_bytes = unsafe { std::slice::from_raw_parts(data_ptr as *const u8, stride).to_vec() };
    migrated_data.insert(comp_id, new_bytes);

    // 6. Push to New Table
    let target_table = &mut world.tables[target_table_idx];
    let new_row = target_table.entities.len();
    target_table.entities.push(entity_id);

    for (cid, bytes) in migrated_data {
        if let Some(col) = target_table.columns.get_mut(&cid) {
            unsafe {
                col.push(bytes.as_ptr());
            }
        }
    }

    world
        .entity_index
        .insert(entity_id, (target_table_idx, new_row));
}

#[no_mangle]
pub extern "C" fn query_archetypes(
    req_ptr: i32,
    req_count: i32,
    out_ptr: i32,
    out_cap: i32,
) -> i32 {
    let world = WORLD.lock().unwrap();
    let required = unsafe { std::slice::from_raw_parts(req_ptr as *const i32, req_count as usize) };
    let out_slice =
        unsafe { std::slice::from_raw_parts_mut(out_ptr as *mut i32, out_cap as usize) };

    let mut count = 0;
    for (idx, table) in world.tables.iter().enumerate() {
        if required.iter().all(|id| table.columns.contains_key(id)) {
            if count < out_cap as usize {
                out_slice[count] = idx as i32;
            }
            count += 1;
        }
    }
    count as i32
}

#[no_mangle]
pub extern "C" fn get_table_column(table_idx: i32, comp_id: i32) -> i64 {
    let mut world = WORLD.lock().unwrap();
    if let Some(table) = world.tables.get_mut(table_idx as usize) {
        if let Some(col) = table.columns.get_mut(&comp_id) {
            let ptr = col.data.as_mut_ptr() as i32;
            let len = col.data.len() as i32;
            return ((len as i64) << 32) | (ptr as i64 & 0xFFFFFFFF);
        }
    }
    0
}

#[no_mangle]
pub extern "C" fn get_standard_ids() -> i64 {
    unsafe {
        let ids = StandardIds {
            position_id: POS_ID,
            tile_id: TILE_ID,
            cursor_id: CURSOR_ID,
        };

        // Allocate memory in WASM for the struct so Host can read it
        let ptr = host_alloc(std::mem::size_of::<StandardIds>() as i32);
        let slice = std::slice::from_raw_parts_mut(ptr as *mut StandardIds, 1);
        slice[0] = ids;

        ptr as i64
    }
}

#[no_mangle]
pub extern "C" fn set_standard_id(kind: i32, id: i32) {
    unsafe {
        match kind {
            1 => POS_ID = id,    // Position
            2 => TILE_ID = id,   // Tile
            3 => CURSOR_ID = id, // Cursor
            _ => {}
        }
    }
}

#[no_mangle]
pub extern "C" fn rebuild_schedule() {
    let mut world = WORLD.lock().unwrap();
    let mut stage = Stage {
        systems: Vec::new(),
    };
    for meta in world.systems.values() {
        stage.systems.push(meta.func);
    }
    world.schedule = vec![stage];
}

#[no_mangle]
pub extern "C" fn run_schedule() {
    // 1. Snapshot functions to run (Unlock Mutex immediately)
    let systems_to_run = {
        let world = WORLD.lock().unwrap();
        let mut funcs = Vec::new();
        for stage in &world.schedule {
            for sys in &stage.systems {
                funcs.push(*sys);
            }
        }
        funcs
    };

    // 2. Execute (re-entrant safe)
    for sys in systems_to_run {
        sys(0);
    }
}

#[no_mangle]
pub extern "C" fn tick(delta: f32) {
    // In a real scenario, we might put 'delta' into a Singleton Resource here

    // Run the schedule (Logic Systems)
    run_schedule();
}
