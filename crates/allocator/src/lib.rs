use std::alloc::{GlobalAlloc, Layout};

extern "C" {
    fn host_alloc(size: i32) -> i32;
    fn host_dealloc(ptr: i32, size: i32);
}

pub struct HostAllocator;

unsafe impl GlobalAlloc for HostAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        if size == 0 {
            return core::ptr::null_mut();
        }
        // Ask Host for an offset
        let ptr = host_alloc(size as i32);
        ptr as *mut u8
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        host_dealloc(ptr as i32, layout.size() as i32);
    }
}

// 3. Set as the Global Allocator for any crate that uses this
// #[global_allocator]
// static ALLOCATOR: HostAllocator = HostAllocator;