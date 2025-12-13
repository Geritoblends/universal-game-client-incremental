// crates/ecs-client/src/lib.rs
use std::alloc::{GlobalAlloc, Layout};
use std::marker::PhantomData;

// --- MEMORY SAFETY ---
mod ffi {
    extern "C" {
        pub fn host_alloc(size: i32) -> i32;
        pub fn host_dealloc(ptr: i32, size: i32);
        pub fn host_print(ptr: i32, len: i32);

        // Core Direct Exports
        pub fn register_component(id: i32, size: i32, align: i32);
        pub fn query_archetypes(req_ptr: i32, req_count: i32, out_ptr: i32, out_cap: i32) -> i32;
        pub fn get_table_column(table_idx: i32, comp_id: i32) -> i64;
        pub fn spawn_entity() -> i32;
        pub fn add_component(entity_id: i32, comp_id: i32, data_ptr: i32);

        // HOST LINKER (The bridge)
        pub fn host_link_call(
            target_mod_ptr: i32,
            target_mod_len: i32,
            target_func_ptr: i32,
            target_func_len: i32,
            local_fn_idx: i32,
            payload_ptr: i32,
            payload_len: i32,
        );
    }
}

struct HostAllocator;
unsafe impl GlobalAlloc for HostAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = ffi::host_alloc(layout.size() as i32);
        if ptr == 0 {
            return std::ptr::null_mut();
        }
        ptr as *mut u8
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        ffi::host_dealloc(ptr as i32, layout.size() as i32);
    }
}

#[global_allocator]
static ALLOCATOR: HostAllocator = HostAllocator;

pub fn print(msg: &str) {
    unsafe {
        ffi::host_print(msg.as_ptr() as i32, msg.len() as i32);
    }
}

// --- PUBLIC API ---

pub trait Component: Sized + 'static {
    const ID: i32;
}

pub fn spawn_entity() -> i32 {
    unsafe { ffi::spawn_entity() }
}

pub fn add_component<T: Component>(entity: i32, component: &T) {
    unsafe {
        ffi::add_component(entity, T::ID, component as *const T as i32);
    }
}

pub fn register_component<T: Component>() {
    unsafe {
        ffi::register_component(
            T::ID,
            std::mem::size_of::<T>() as i32,
            std::mem::align_of::<T>() as i32,
        );
    }
}

pub fn register_system<Q: WorldQuery>(name: &str, sys_fn: extern "C" fn(i32)) {
    let target_mod = "Core";
    let target_hook = "_link_system"; // Maps to the Core function above

    unsafe {
        ffi::host_link_call(
            target_mod.as_ptr() as i32,
            target_mod.len() as i32,
            target_hook.as_ptr() as i32,
            target_hook.len() as i32,
            sys_fn as usize as i32,
            name.as_ptr() as i32,
            name.len() as i32,
        );
    }
}

// --- QUERY SYSTEM ---

pub struct ColumnView<T> {
    ptr: *mut T,
    _len: usize,
}

pub trait WorldQuery {
    type Item<'a>;
    type Columns;
    fn get_ids() -> Vec<i32>;
    unsafe fn init_columns(table_idx: i32) -> Self::Columns;
    unsafe fn fetch<'a>(columns: &Self::Columns, row: usize) -> Self::Item<'a>;
}

pub struct Query<Q: WorldQuery>(PhantomData<Q>);

impl<Q: WorldQuery> Query<Q> {
    pub fn new() -> Self {
        Self(PhantomData)
    }

    pub fn for_each<F>(&self, mut f: F)
    where
        F: FnMut(Q::Item<'_>),
    {
        let ids = Q::get_ids();
        let mut table_indices = [0i32; 64];
        let count = unsafe {
            ffi::query_archetypes(
                ids.as_ptr() as i32,
                ids.len() as i32,
                table_indices.as_mut_ptr() as i32,
                64,
            )
        };

        for i in 0..count {
            let table_idx = table_indices[i as usize];
            unsafe {
                let columns = Q::init_columns(table_idx);
                // We use ID[0] to determine row count
                let packed = ffi::get_table_column(table_idx, ids[0]);
                let len_bytes = (packed >> 32) as usize;
                // Note: You need a way to know the size of component ID[0] here to calc rows perfectly.
                // For now assuming 8 bytes (f32, f32). In prod, `get_table_column` should return `rows` directly.
                let row_count = len_bytes / 8;

                for row in 0..row_count {
                    f(Q::fetch(&columns, row));
                }
            }
        }
    }
}

// --- IMPLS ---

impl<T: Component> WorldQuery for &mut T {
    type Item<'a> = &'a mut T;
    type Columns = ColumnView<T>;
    fn get_ids() -> Vec<i32> {
        vec![T::ID]
    }
    unsafe fn init_columns(idx: i32) -> Self::Columns {
        let packed = ffi::get_table_column(idx, T::ID);
        let ptr = (packed & 0xFFFFFFFF) as *mut T;
        ColumnView { ptr, _len: 0 }
    }
    unsafe fn fetch<'a>(col: &Self::Columns, row: usize) -> Self::Item<'a> {
        &mut *col.ptr.add(row)
    }
}

impl<T: Component> WorldQuery for &T {
    type Item<'a> = &'a T;
    type Columns = ColumnView<T>;
    fn get_ids() -> Vec<i32> {
        vec![T::ID]
    }
    unsafe fn init_columns(idx: i32) -> Self::Columns {
        let packed = ffi::get_table_column(idx, T::ID);
        let ptr = (packed & 0xFFFFFFFF) as *mut T;
        ColumnView { ptr, _len: 0 }
    }
    unsafe fn fetch<'a>(col: &Self::Columns, row: usize) -> Self::Item<'a> {
        &*col.ptr.add(row)
    }
}

impl<A: WorldQuery, B: WorldQuery> WorldQuery for (A, B) {
    type Item<'a> = (A::Item<'a>, B::Item<'a>);
    type Columns = (A::Columns, B::Columns);
    fn get_ids() -> Vec<i32> {
        let mut ids = A::get_ids();
        ids.extend(B::get_ids());
        ids
    }
    unsafe fn init_columns(idx: i32) -> Self::Columns {
        (A::init_columns(idx), B::init_columns(idx))
    }
    unsafe fn fetch<'a>((a, b): &Self::Columns, row: usize) -> Self::Item<'a> {
        (A::fetch(a, row), B::fetch(b, row))
    }
}
