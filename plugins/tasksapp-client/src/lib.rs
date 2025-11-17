use tasksapp_net::{NewTaskRequest, NewTaskResult, QueryByIdResult, Task};

// Declare host functions that we'll import
unsafe extern "C" {
    fn call(
        instance_id_ptr: i32,
        instance_id_len: i32,
        func_name_ptr: i32,
        func_name_len: i32,
        payload_ptr: i32,
        payload_len: i32,
    ) -> (i32, i32);

    fn send_to_server(message_ptr: i32, message_len: i32);

    fn fire_and_forget(
        instance_id_ptr: i32,
        instance_id_len: i32,
        func_ptr: i32,
        func_len: i32,
        payload_ptr: i32,
        payload_len: i32,
    );
}

// Helper function to call into tasksapp_core via the host
fn call_core(func_name: &str, payload: &[u8]) -> Vec<u8> {
    let instance_id = b"tasksapp_core";
    let func_name_bytes = func_name.as_bytes();

    let (result_ptr, result_len) = unsafe {
        call(
            instance_id.as_ptr() as i32,
            instance_id.len() as i32,
            func_name_bytes.as_ptr() as i32,
            func_name_bytes.len() as i32,
            payload.as_ptr() as i32,
            payload.len() as i32,
        )
    };

    // Read result from shared memory - THIS WORKS because we share memory!
    unsafe {
        let result_slice = std::slice::from_raw_parts(result_ptr as *const u8, result_len as usize);
        result_slice.to_vec()
    }
}

#[unsafe(no_mangle)]
pub fn create_task(title_ptr: i32, title_len: i32, priority: u8, result_ptr: i32) {
    // 1. Read title (Same as before)
    let title = unsafe {
        let slice = std::slice::from_raw_parts(title_ptr as *const u8, title_len as usize);
        String::from_utf8_lossy(slice).to_string()
    };

    // 2. Create request (Same as before)
    let request = NewTaskRequest {
        title,
        priority,
        completed: false,
    };

    // 3. Logic (Same as before)
    let payload = bincode::serialize(&request).unwrap();
    let result_bytes = call_core("new_task", &payload);
    let result: NewTaskResult = bincode::deserialize(&result_bytes).unwrap();
    let response = bincode::serialize(&result).unwrap();

    // 4. Prepare return values
    let ptr = response.as_ptr() as i32;
    let len = response.len() as i32;
    std::mem::forget(response);

    // 5. FIX: Write the result manually to the result_ptr provided by the host
    unsafe {
        let result_slice = std::slice::from_raw_parts_mut(result_ptr as *mut i32, 2);
        result_slice[0] = ptr; // Write ptr at offset 0
        result_slice[1] = len; // Write len at offset 4
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn list_pending_tasks() -> (i32, i32) {
    let result_bytes = call_core("show_pending_tasks", &[]);

    let tasks: Vec<Task> = bincode::deserialize(&result_bytes).unwrap();

    let response = bincode::serialize(&tasks).unwrap();
    let ptr = response.as_ptr() as i32;
    let len = response.len() as i32;
    std::mem::forget(response);

    (ptr, len)
}

#[unsafe(no_mangle)]
pub extern "C" fn list_completed_tasks() -> (i32, i32) {
    let result_bytes = call_core("show_completed_tasks", &[]);

    let tasks: Vec<Task> = bincode::deserialize(&result_bytes).unwrap();

    let response = bincode::serialize(&tasks).unwrap();
    let ptr = response.as_ptr() as i32;
    let len = response.len() as i32;
    std::mem::forget(response);

    (ptr, len)
}

#[unsafe(no_mangle)]
pub extern "C" fn complete_task(task_id: i32) {
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
pub extern "C" fn get_task(task_id: i32) -> (i32, i32) {
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
pub extern "C" fn sync_to_server() {
    let message = b"Syncing tasks to server...";

    unsafe {
        send_to_server(message.as_ptr() as i32, message.len() as i32);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dealloc(ptr: i32, len: i32) {
    unsafe {
        let _ = Vec::from_raw_parts(ptr as *mut u8, len as usize, len as usize);
    }
}
