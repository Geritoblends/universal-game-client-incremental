// lib.rs
use once_cell::sync::Lazy;
use std::sync::Mutex;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Task {
    pub title: String,
    pub priority: u8,
}

static TODOAPP: Lazy<Mutex<Vec<Task>>> = Lazy::new(|| Mutex::new(Vec::new()));

/// Allocate memory in WASM module
#[no_mangle]
pub extern "C" fn allocate(size: u32) -> *mut u8 {
    let mut buf = Vec::with_capacity(size as usize);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// Deallocate memory in WASM module
#[no_mangle]
pub extern "C" fn deallocate(ptr: *mut u8, size: u32) {
    unsafe { Vec::from_raw_parts(ptr, size as usize, size as usize); }
}

/// Create a new task from a JSON string passed by the host
#[no_mangle]
pub extern "C" fn new_task(ptr: i32, len: i32) -> i32 {
    let slice = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let json = match std::str::from_utf8(slice) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    let task: Task = match serde_json::from_str(json) {
        Ok(t) => t,
        Err(_) => return 0,
    };
    TODOAPP.lock().unwrap().push(task);
    1
}

/// Return pending tasks as a pointer and length tuple
#[no_mangle]
pub extern "C" fn pending_tasks() -> (i32, i32) {
    let data = serde_json::to_vec(&*TODOAPP.lock().unwrap()).unwrap();
    let len = data.len();
    let ptr = allocate(len as u32);
    unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, len); }
    std::mem::forget(data);
    (ptr as i32, len as i32)
}
