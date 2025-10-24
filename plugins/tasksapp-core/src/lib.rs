pub mod todoapp;

use once_cell::sync::Lazy;
use std::sync::Mutex;
use crate::{Priority, Task, TodoApp};

static TODOAPP: Lazy<TodoApp> = Lazy::new(|| TodoApp::new());

#[no_mangle]
pub extern "C" fn allocate(size: u32) -> *mut u8 {
    let mut buf = Vec::with_capacity(size as usize);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

#[no_mangle]
pub extern "C" fn deallocate(ptr: *mut u8, size: u32) {
    unsafe {
        Vec::from_raw_parts(ptr, size as usize, size as usize);
    }
}

#[no_mangle]
pub extern "C" fn new_task(task_ptr: i32, task_len: i32) -> i32  {
    unimplemented!();
}

#[no_mangle]
pub extern "C" fn pending_tasks() -> *const u8 {
    unimplemented!();
}
