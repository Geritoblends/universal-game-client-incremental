// reporter/src/lib.rs
#[no_mangle]
pub extern "C" fn allocate(size: u32) -> *mut u8 {
    let mut buf = Vec::with_capacity(size as usize);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

#[no_mangle]
pub extern "C" fn deallocate(ptr: *mut u8, size: u32) {
    unsafe { Vec::from_raw_parts(ptr, size as usize, size as usize); }
}

/// Receive pending tasks JSON from the host and process them
#[no_mangle]
pub extern "C" fn handle_tasks(ptr: i32, len: i32) {
    let slice = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    if let Ok(json) = std::str::from_utf8(slice) {
        println!("Plugin B received tasks: {}", json);
        // Here you could deserialize JSON into Task structs if needed
        // let tasks: Vec<Task> = serde_json::from_str(json).unwrap();
    }
}
