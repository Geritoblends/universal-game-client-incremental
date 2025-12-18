use bevy_ecs::component::{ComponentDescriptor, ComponentId, StorageType};
use bevy_ecs::prelude::*;
use bevy_ptr::OwningPtr;
use getrandom::{register_custom_getrandom, Error};
use std::alloc::{GlobalAlloc, Layout};
use std::ptr::NonNull;
use std::slice;

fn custom_getrandom(buf: &mut [u8]) -> Result<(), Error> {
    // Just fill with a pattern (not secure, but fine for game HashMaps)
    for (i, byte) in buf.iter_mut().enumerate() {
        *byte = i as u8;
    }
    Ok(())
}

register_custom_getrandom!(custom_getrandom);
// --------------------------
// ============================================================================
// 1. HOST MEMORY INTERFACE
// ============================================================================
// We delegate all allocation to the Host (Rust) so memory is shared cleanly.

struct HostAllocator;

extern "C" {
    fn host_alloc(size: i32) -> i32;
    fn host_dealloc(ptr: i32, size: i32);
}

unsafe impl GlobalAlloc for HostAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        host_alloc(layout.size() as i32) as *mut u8
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        host_dealloc(ptr as i32, layout.size() as i32);
    }
}

#[global_allocator]
static ALLOCATOR: HostAllocator = HostAllocator;

// ============================================================================
// 2. KERNEL STATE
// ============================================================================

static mut WORLD: Option<World> = None;
static mut COMPONENT_MAP: Vec<ComponentId> = Vec::new();

// Storage for dynamic Resources (Just raw blobs of memory on the heap)
static mut RESOURCES: Vec<Option<Box<[u8]>>> = Vec::new();

// Re-usable buffer to return query results (avoids allocation per frame)
static mut QUERY_BUFFER: Vec<i32> = Vec::new();

// ============================================================================
// 3. SYSTEM CALLS (The API)
// ============================================================================

#[no_mangle]
pub extern "C" fn kernel_init() {
    unsafe {
        if WORLD.is_none() {
            WORLD = Some(World::new());
        }
    }
}

// --- COMPONENT REGISTRATION ---

/// Registers a component type with a specific size/alignment.
/// Returns a unique Integer ID for this component.
#[no_mangle]
pub extern "C" fn sys_register_component(size: i32, align: i32) -> i32 {
    let world = unsafe { WORLD.as_mut().unwrap() };

    // Create a descriptor for a Table-stored component of this layout
    let layout = Layout::from_size_align(size as usize, align as usize).unwrap();
    let descriptor = ComponentDescriptor::new(StorageType::Table, layout, None);

    let id = world.register_component(descriptor);

    unsafe {
        COMPONENT_MAP.push(id);
        (COMPONENT_MAP.len() - 1) as i32
    }
}

// --- ENTITY MANAGEMENT ---

/// Spawns an entity with a list of components.
/// `comp_ids_ptr`: Array of IDs returned by sys_register_component
/// `data_ptrs`: Array of pointers to the component data to copy
#[no_mangle]
pub extern "C" fn sys_spawn_entity(
    count: i32,
    comp_ids_ptr: *const i32,
    data_ptrs: *const *const u8,
) -> i32 {
    let world = unsafe { WORLD.as_mut().unwrap() };

    // 1. Spawn Empty
    let mut entity_cmds = world.spawn_empty();
    let e_id = entity_cmds.id();

    // 2. Insert Components safely
    let ids = unsafe { slice::from_raw_parts(comp_ids_ptr, count as usize) };
    let ptrs = unsafe { slice::from_raw_parts(data_ptrs, count as usize) };

    for i in 0..count as usize {
        let internal_id = unsafe { COMPONENT_MAP[ids[i] as usize] };
        let raw_data_ptr = ptrs[i];

        unsafe {
            // Bevy's OwningPtr tells the World: "Take ownership of the bytes at this pointer"
            // Since we are copying from Guest stack to Kernel heap, this is effectively a copy.
            let ptr = OwningPtr::new(NonNull::new(raw_data_ptr as *mut u8).unwrap());
            world.entity_mut(e_id).insert_by_id(internal_id, ptr);
        }
    }

    e_id.index() as i32
}

// --- QUERIES ---

/// Finds all Tables that match the list of component IDs.
/// Writes result length to `out_len` and returns pointer to the list of TableIDs.
#[no_mangle]
pub extern "C" fn sys_query_tables(
    req_ids_ptr: *const i32,
    req_len: i32,
    out_len: *mut i32,
) -> *const i32 {
    let world = unsafe { WORLD.as_mut().unwrap() };
    let req_indices = unsafe { slice::from_raw_parts(req_ids_ptr, req_len as usize) };

    unsafe {
        QUERY_BUFFER.clear();

        // Convert plugin IDs to Bevy ComponentIds
        // (In a real app, you'd cache the Archetype generation, but scanning tables is okay for small games)
        let required_comps: Vec<ComponentId> = req_indices
            .iter()
            .map(|&idx| COMPONENT_MAP[idx as usize])
            .collect();

        for table in world.storages().tables.iter() {
            if required_comps.iter().all(|&c| table.has_component(c)) {
                QUERY_BUFFER.push(table.id().index() as i32);
            }
        }

        *out_len = QUERY_BUFFER.len() as i32;
        QUERY_BUFFER.as_ptr()
    }
}

/// Returns the number of entities in a Table
#[no_mangle]
pub extern "C" fn sys_get_table_len(table_id: i32) -> i32 {
    let world = unsafe { WORLD.as_ref().unwrap() };
    let t_id = bevy_ecs::storage::TableId::new(table_id as usize);
    match world.storages().tables.get(t_id) {
        Some(t) => t.len() as i32,
        None => 0,
    }
}

/// Returns the raw pointer to the start of the component column array.
#[no_mangle]
pub extern "C" fn sys_get_column_ptr(table_id: i32, comp_index: i32) -> *mut u8 {
    let world = unsafe { WORLD.as_mut().unwrap() }; // Mut access needed for ptr
    let t_id = bevy_ecs::storage::TableId::new(table_id as usize);
    let c_id = unsafe { COMPONENT_MAP[comp_index as usize] };

    if let Some(table) = world.storages().tables.get(t_id) {
        if let Some(column) = table.get_column(c_id) {
            return column.get_data_ptr().as_ptr();
        }
    }
    std::ptr::null_mut()
}

// --- RESOURCES ---

/// Gets a pointer to a Resource blob.
/// If it doesn't exist and `size` > 0, it allocates it.
#[no_mangle]
pub extern "C" fn sys_resource(id: i32, size: i32) -> *mut u8 {
    unsafe {
        let idx = id as usize;

        // 1. Expansion
        if RESOURCES.len() <= idx {
            if size == 0 {
                // Host asking for non-existent resource? Return NULL.
                return std::ptr::null_mut();
            }
            RESOURCES.resize(idx + 1, None);
        }

        // 2. Allocation
        if RESOURCES[idx].is_none() {
            if size > 0 {
                let vec = vec![0u8; size as usize];
                RESOURCES[idx] = Some(vec.into_boxed_slice());
            } else {
                return std::ptr::null_mut();
            }
        }

        // 3. Access
        match &mut RESOURCES[idx] {
            Some(blob) => blob.as_mut_ptr(),
            None => std::ptr::null_mut(),
        }
    }
}
