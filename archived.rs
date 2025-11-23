#[unsafe(no_mangle)]
pub fn list_pending_tasks() -> (i32, i32) {
    let result_bytes = call_core("show_pending_tasks", &[]);

    let tasks: Vec<Task> = bincode::deserialize(&result_bytes).unwrap();

    let response = bincode::serialize(&tasks).unwrap();
    let ptr = response.as_ptr() as i32;
    let len = response.len() as i32;
    std::mem::forget(response);

    (ptr, len)
}

#[unsafe(no_mangle)]
pub fn list_completed_tasks() -> (i32, i32) {
    let result_bytes = call_core("show_completed_tasks", &[]);

    let tasks: Vec<Task> = bincode::deserialize(&result_bytes).unwrap();

    let response = bincode::serialize(&tasks).unwrap();
    let ptr = response.as_ptr() as i32;
    let len = response.len() as i32;
    std::mem::forget(response);

    (ptr, len)
}

#[unsafe(no_mangle)]
pub fn complete_task(task_id: i32) {
    let payload = bincode::serialize(&task_id).unwrap();

    let instance_id = b"tasksapp_core";
    let func_name = b"mark_as_completed";

    unsafe {
        fire_and_forget(
            instance_id.as_ptr() as i32,
            instance_id.len() as i32,
            func_name.as_ptr() as i32,
            func_name.len() as i32,
            payload.as_ptr() as i32,
            payload.len() as i32,
        );
    }
}

#[unsafe(no_mangle)]
pub fn get_task(task_id: i32) -> (i32, i32) {
    let payload = bincode::serialize(&task_id).unwrap();

    let result_bytes = call_core("query_by_id", &payload);

    let result: QueryByIdResult = bincode::deserialize(&result_bytes).unwrap();

    let response = bincode::serialize(&result).unwrap();
    let ptr = response.as_ptr() as i32;
    let len = response.len() as i32;
    std::mem::forget(response);

    (ptr, len)
}

#[unsafe(no_mangle)]
pub fn sync_to_server() {
    let message = b"Syncing tasks to server...";

    unsafe {
        send_to_server(message.as_ptr() as i32, message.len() as i32);
    }
}

#[unsafe(no_mangle)]
pub fn dealloc(ptr: i32, len: i32) {
    unsafe {
        let _ = Vec::from_raw_parts(ptr as *mut u8, len as usize, len as usize);
    }
}
