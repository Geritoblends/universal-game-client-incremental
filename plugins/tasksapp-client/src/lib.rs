use std::alloc::{GlobalAlloc, Layout};
use tasksapp_net::{NewTaskRequest, NewTaskResult};

// ============================================================================
//  THE SYSTEM ALLOCATOR (The Magic Fix)
//  Redirects all heap allocations to the Host to prevent collision/freezes.
// ============================================================================

unsafe extern "C" {
    // New imports for Memory Management
    fn host_alloc(size: i32) -> i32;
    fn host_dealloc(ptr: i32, size: i32);

    // Existing imports
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

struct HostAllocator;

unsafe impl GlobalAlloc for HostAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // Ask Host for memory. The result is an offset in Shared Memory.
        // In Wasm Linear Memory, Offset == Pointer.
        let ptr = host_alloc(layout.size() as i32);
        ptr as *mut u8
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        host_dealloc(ptr as i32, layout.size() as i32);
    }
}

// Replace the default 'dlmalloc' with our Host Allocator
#[global_allocator]
static ALLOCATOR: HostAllocator = HostAllocator;

// ============================================================================
//  BUSINESS LOGIC
// ============================================================================

fn print(s: &str) {
    unsafe {
        host_print(s.as_ptr() as i32, s.len() as i32);
    }
}

fn pack_i64(ptr: i32, len: i32) -> i64 {
    (len as i64) << 32 | (ptr as i64 & 0xFFFFFFFF)
}

fn call_core(func_name: &str, payload: &[u8]) -> (i32, i32) {
    let instance_id = b"tasksapp_core".to_vec();
    let func_name_bytes = func_name.as_bytes();

    print(&format!("Calling core with: {}", func_name));

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

    print(&format!("call_core returned i64: {}", packed_result));

    let ptr: i32 = (packed_result & 0xFFFFFFFF) as i32;
    let len: i32 = (packed_result >> 32) as i32;
    (ptr, len)
}

#[unsafe(no_mangle)]
pub fn create_task(title_ptr: i32, title_len: i32, priority: i32) -> i64 {
    print(&"Hello from client create_task".to_string());

    // 1. Read title
    // Because of the HostAllocator, this String is allocated in safe memory
    let title = unsafe {
        let slice = std::slice::from_raw_parts(title_ptr as *const u8, title_len as usize);
        String::from_utf8_lossy(slice).to_string()
    };

    print(&format!("title: {}", title));

    // 2. Create request
    let request = NewTaskRequest {
        title,
        priority,
        completed: false,
    };

    print(&format!("sending request: {:#?}", request));

    // 3. Call core
    let payload = bincode::serialize(&request).unwrap();
    print(&"bincode serialize works");

    let (result_ptr, result_len) = call_core("new_task", &payload);
    print(&"call_core works");

    // 4. Read result
    let result_bytes =
        unsafe { std::slice::from_raw_parts(result_ptr as *const u8, result_len as usize) };

    let result: NewTaskResult = bincode::deserialize(result_bytes).expect("error deserializing");

    let debug: String = format!("{:?}", result);
    print(&debug);

    // 6. Return response
    let response = bincode::serialize(&result).unwrap();
    let ptr = response.as_ptr() as i32;
    let len = response.len() as i32;
    std::mem::forget(response); // Leak it to the host

    pack_i64(ptr, len)
}

// Implement other exports (list_pending_tasks) similarly if needed...
#[unsafe(no_mangle)]
pub fn list_pending_tasks() -> i64 {
    let (result_ptr, result_len) = call_core("show_pending_tasks", &[]);
    pack_i64(result_ptr, result_len)
}
