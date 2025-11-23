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
    ) -> i64;

    fn host_print(ptr: i32, len: i32);

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

fn print(s: &str) {
    unsafe {
        host_print(s.as_ptr() as i32, s.len() as i32);
    }
}

// You'll also need this helper function:
fn pack_i64(ptr: i32, len: i32) -> i64 {
    (len as i64) << 32 | (ptr as i64 & 0xFFFFFFFF)
}

// Helper function to call into tasksapp_core via the host
fn call_core(func_name: &str, payload: &[u8]) -> (i32, i32) {
    let instance_id = b"tasksapp_core";
    let func_name_bytes = func_name.as_bytes();

    let packed_result = unsafe {
        call(
            instance_id.as_ptr() as i32,
            instance_id.len() as i32,
            func_name_bytes.as_ptr() as i32,
            func_name_bytes.len() as i32,
            payload.as_ptr() as i32,
            payload.len() as i32,
        )
    };

    // Unpack i64 back to (i32, i32)
    let ptr: i32 = (packed_result & 0xFFFFFFFF) as i32;
    let len: i32 = (packed_result >> 32) as i32;
    (ptr, len)
}

#[unsafe(no_mangle)]
pub fn create_task(title_ptr: i32, title_len: i32, priority: i32) -> i64 {
    // 1. Read title
    let title = unsafe {
        let slice = std::slice::from_raw_parts(title_ptr as *const u8, title_len as usize);
        String::from_utf8_lossy(slice).to_string()
    };

    // 2. Create request
    let request = NewTaskRequest {
        title,
        priority,
        completed: false,
    };

    // 3. Call core and get (result_ptr, result_len)
    let payload = bincode::serialize(&request).unwrap();
    let (result_ptr, result_len) = call_core("new_task", &payload);
    println!("new_task correctly called");

    // 4. Read the result data from shared memory
    let result_bytes =
        unsafe { std::slice::from_raw_parts(result_ptr as *const u8, result_len as usize) };

    // 5. Deserialize the result
    let result: NewTaskResult = bincode::deserialize(result_bytes).expect("error deserializing");

    let debug: String = format!("{:?}", result);
    print(&debug);

    // 6. Create our response and pack it
    let response = bincode::serialize(&result).unwrap();
    let ptr = response.as_ptr() as i32;
    let len = response.len() as i32;
    std::mem::forget(response);

    // 7. Pack as i64
    pack_i64(ptr, len)
}
