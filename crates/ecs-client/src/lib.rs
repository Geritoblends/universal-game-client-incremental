use std::alloc::{GlobalAlloc, Layout};
use std::marker::PhantomData;
use std::sync::atomic::{AtomicI32, Ordering};

// ============================================================================
// 1. HOST & KERNEL BINDS
// ============================================================================

struct HostAllocator;
extern "C" {
    fn host_alloc(size: i32) -> i32;
    fn host_dealloc(ptr: i32, size: i32);

    // Kernel Syscalls
    fn sys_register_component(size: i32, align: i32) -> i32;
    fn sys_spawn_entity(count: i32, ids: *const i32, data: *const *const u8) -> i32;
    fn sys_query_tables(ids: *const i32, len: i32, out_len: *mut i32) -> *const i32;
    fn sys_get_table_len(table: i32) -> i32;
    fn sys_get_column_ptr(table: i32, comp: i32) -> *mut u8;
    fn sys_resource(id: i32, size: i32) -> *mut u8;
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
// 2. COMPONENTS & COMMANDS
// ============================================================================

pub trait Component: Sized + 'static {
    fn get_id() -> i32 {
        static ID: AtomicI32 = AtomicI32::new(-1);
        let id = ID.load(Ordering::Relaxed);
        if id == -1 {
            let new_id = unsafe {
                sys_register_component(
                    std::mem::size_of::<Self>() as i32,
                    std::mem::align_of::<Self>() as i32,
                )
            };
            ID.store(new_id, Ordering::Relaxed);
            return new_id;
        }
        id
    }
}

// Support tuples for spawning
pub trait Bundle {
    fn get_ids_and_ptrs(&self, ids: &mut Vec<i32>, ptrs: &mut Vec<*const u8>);
}

// Impl Bundle for single component
impl<T: Component> Bundle for T {
    fn get_ids_and_ptrs(&self, ids: &mut Vec<i32>, ptrs: &mut Vec<*const u8>) {
        ids.push(T::get_id());
        ptrs.push(self as *const T as *const u8);
    }
}

// Impl Bundle for tuple (A, B)
impl<A: Component, B: Component> Bundle for (A, B) {
    fn get_ids_and_ptrs(&self, ids: &mut Vec<i32>, ptrs: &mut Vec<*const u8>) {
        ids.push(A::get_id());
        ptrs.push(&self.0 as *const A as *const u8);
        ids.push(B::get_id());
        ptrs.push(&self.1 as *const B as *const u8);
    }
}

pub struct Commands;
impl Commands {
    pub fn spawn<B: Bundle>(bundle: B) {
        let mut ids = Vec::new();
        let mut ptrs = Vec::new();
        bundle.get_ids_and_ptrs(&mut ids, &mut ptrs);

        unsafe {
            sys_spawn_entity(ids.len() as i32, ids.as_ptr(), ptrs.as_ptr());
        }
    }
}

// ============================================================================
// 3. RESOURCES
// ============================================================================

pub trait Resource: Sized + 'static {
    // We change this to a function that CAN be overridden
    fn resource_id() -> i32 {
        // Default behavior: Generate a random ID (offset by 1000 to avoid conflicts with fixed IDs)
        static ID: AtomicI32 = AtomicI32::new(-1);
        let id = ID.load(Ordering::Relaxed);
        if id == -1 {
            static CTR: AtomicI32 = AtomicI32::new(1000);
            let new_id = CTR.fetch_add(1, Ordering::Relaxed);
            ID.store(new_id, Ordering::Relaxed);
            return new_id;
        }
        id
    }
}

// Accessors
pub struct Res<'a, T: Resource> {
    ptr: *const T,
    _m: PhantomData<&'a T>,
}
impl<'a, T: Resource> Res<'a, T> {
    pub fn get() -> Self {
        unsafe {
            let ptr = sys_resource(T::resource_id(), std::mem::size_of::<T>() as i32);
            Self {
                ptr: ptr as *const T,
                _m: PhantomData,
            }
        }
    }
}
impl<'a, T: Resource> std::ops::Deref for Res<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.ptr }
    }
}

pub struct ResMut<'a, T: Resource> {
    ptr: *mut T,
    _m: PhantomData<&'a mut T>,
}
impl<'a, T: Resource> ResMut<'a, T> {
    pub fn get() -> Self {
        unsafe {
            let ptr = sys_resource(T::resource_id(), std::mem::size_of::<T>() as i32);
            Self {
                ptr: ptr as *mut T,
                _m: PhantomData,
            }
        }
    }
}
impl<'a, T: Resource> std::ops::Deref for ResMut<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.ptr }
    }
}
impl<'a, T: Resource> std::ops::DerefMut for ResMut<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.ptr }
    }
}

#[macro_export]
macro_rules! export_grid {
    ($grid_type:ty) => {
        /// The Host calls this to get the pointer.
        #[no_mangle]
        pub extern "C" fn get_grid_ptr() -> i32 {
            // Force creation if it doesn't exist
            let res = $crate::Res::<$grid_type>::get();
            // Return raw Wasm pointer (u32 cast to i32)
            (res.deref() as *const $grid_type) as i32
        }
    };
}

// ============================================================================
// 4. QUERIES
// ============================================================================

pub struct Query<T> {
    _m: PhantomData<T>,
}

impl<T: Component> Query<T> {
    pub fn new() -> Self {
        Self { _m: PhantomData }
    }

    pub fn for_each<F>(&self, mut f: F)
    where
        F: FnMut(&mut T),
    {
        unsafe {
            let cid = T::get_id();
            let reqs = [cid];
            let mut count = 0;

            // 1. Get Tables
            let tables_ptr = sys_query_tables(reqs.as_ptr(), 1, &mut count);
            let tables = std::slice::from_raw_parts(tables_ptr, count as usize);

            for &tid in tables {
                // 2. Get Data
                let len = sys_get_table_len(tid);
                let ptr = sys_get_column_ptr(tid, cid);

                // 3. Slice & Iterate
                let slice = std::slice::from_raw_parts_mut(ptr as *mut T, len as usize);
                for item in slice {
                    f(item);
                }
            }
        }
    }
}

// Tuple Query support (A, B)
impl<A: Component, B: Component> Query<(A, B)> {
    pub fn new() -> Self {
        Self { _m: PhantomData }
    }

    pub fn for_each<F>(&self, mut f: F)
    where
        F: FnMut(&mut A, &mut B),
    {
        unsafe {
            let id_a = A::get_id();
            let id_b = B::get_id();
            let reqs = [id_a, id_b];
            let mut count = 0;

            let tables_ptr = sys_query_tables(reqs.as_ptr(), 2, &mut count);
            let tables = std::slice::from_raw_parts(tables_ptr, count as usize);

            for &tid in tables {
                let len = sys_get_table_len(tid) as usize;
                let ptr_a = sys_get_column_ptr(tid, id_a) as *mut A;
                let ptr_b = sys_get_column_ptr(tid, id_b) as *mut B;

                let slice_a = std::slice::from_raw_parts_mut(ptr_a, len);
                let slice_b = std::slice::from_raw_parts_mut(ptr_b, len);

                for i in 0..len {
                    f(&mut slice_a[i], &mut slice_b[i]);
                }
            }
        }
    }
}

// ============================================================================
// 5. APP ABSTRACTION
// ============================================================================

pub struct App {
    startup: Vec<fn()>,
    update: Vec<fn()>,
}
impl App {
    pub fn new() -> Self {
        Self {
            startup: vec![],
            update: vec![],
        }
    }
    pub fn add_systems(&mut self, s: Schedule, f: fn()) {
        match s {
            Schedule::Startup => self.startup.push(f),
            Schedule::Update => self.update.push(f),
        }
    }
}
pub enum Schedule {
    Startup,
    Update,
}

#[macro_export]
macro_rules! register_plugin {
    ($setup:ident) => {
        static mut APP: Option<$crate::App> = None;
        #[no_mangle]
        pub extern "C" fn plugin_init() {
            unsafe {
                let mut app = $crate::App::new();
                $setup(&mut app);
                for s in &app.startup {
                    s();
                }
                APP = Some(app);
            }
        }
        #[no_mangle]
        pub extern "C" fn plugin_update() {
            unsafe {
                if let Some(app) = &APP {
                    for s in &app.update {
                        s();
                    }
                }
            }
        }
    };
}
