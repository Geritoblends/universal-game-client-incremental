pub mod todoapp;

use once_cell::sync::Lazy;
use std::sync::Mutex;
use crate::{Priority, Task, TodoApp};

static TODOAPP: Lazy<TodoApp> = Lazy::new(|| TodoApp::new());

#[no_mangle]
pub extern "C" fn allocate(len: i32) -> i32 {
    unimplemented!();
}

#[no_mangle]
pub extern "C" fn new_task(task_ptr: i32, task_len: i32) -> i32  {
    unimplemented!();
}

#[no_mangle]
pub extern "C" fn pending_tasks() -> *const u8 {
    unimplemented!();
}
