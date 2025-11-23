use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;
use std::alloc::{GlobalAlloc, Layout};
use tasksapp_net::{NewTaskError, NewTaskRequest, NewTaskResult, QueryByIdResult, Task};

// --- ALLOCATOR ---
unsafe extern "C" {
    fn host_alloc(size: i32) -> i32;
    fn host_dealloc(ptr: i32, size: i32);
    fn host_print(ptr: i32, len: i32);
}

struct HostAllocator;
unsafe impl GlobalAlloc for HostAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe {
            let ptr = host_alloc(layout.size() as i32);
            ptr as *mut u8
        }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { host_dealloc(ptr as i32, layout.size() as i32); }
    }
}

#[global_allocator]
static ALLOCATOR: HostAllocator = HostAllocator;

// --- HELPERS ---

fn print(s: &str) {
    unsafe { host_print(s.as_ptr() as i32, s.len() as i32); }
}

// FIX 1: Standardize Packing Helper (High=Len, Low=Ptr)
fn pack(ptr: i32, len: i32) -> i64 {
    (len as i64) << 32 | (ptr as i64 & 0xFFFFFFFF)
}

static DB: Lazy<Mutex<HashMap<i32, Task>>> = Lazy::new(|| Mutex::new(HashMap::new()));
static mut CURRENT_ID: i32 = 0;

// --- EXPORTS ---

#[unsafe(no_mangle)]
pub fn new_task(payload_ptr: i32, payload_len: i32) -> i64 {
    print(&format!("Hello from Core!"));
    
    let payload = unsafe {
        let slice = std::slice::from_raw_parts(payload_ptr as *const u8, payload_len as usize);
        slice.to_vec()
    };

    let request: NewTaskRequest = match bincode::deserialize(&payload) {
        Ok(req) => req,
        Err(_) => {
            print(&format!("Error deserializing request"));
            let result = NewTaskResult::Error(NewTaskError::TaskAlreadyExists);
            let serialized = bincode::serialize(&result).unwrap();
            let ptr = serialized.as_ptr() as i32;
            let len = serialized.len() as i32;
            std::mem::forget(serialized);
            return pack(ptr, len); // FIX: Use helper
        }
    };

    let task_id = unsafe {
        CURRENT_ID += 1;
        CURRENT_ID
    };

    let task = Task {
        id: task_id,
        title: request.title,
        priority: request.priority,
        completed: request.completed,
    };

    let mut db = DB.lock().unwrap();
    db.insert(task_id, task.clone());

    let result = NewTaskResult::Success(task);
    
    let serialized = bincode::serialize(&result).unwrap();
    let ptr = serialized.as_ptr() as i32;
    let len = serialized.len() as i32;
    std::mem::forget(serialized);

    pack(ptr, len) // FIX: Use helper
}

// FIX 2: Update Signature to accept arguments (even if unused)
#[unsafe(no_mangle)]
pub fn show_pending_tasks(_ptr: i32, _len: i32) -> i64 {
    let db = DB.lock().unwrap();
    let pending: Vec<Task> = db.values().filter(|t| !t.completed).cloned().collect();

    print(&format!("Tasks:"));
    for task in &pending {
        print(&format!("{:?}", task));
    }

    let serialized = bincode::serialize(&pending).unwrap();
    let ptr = serialized.as_ptr() as i32;
    let len = serialized.len() as i32;
    std::mem::forget(serialized);

    pack(ptr, len) // FIX: Use helper
}

// FIX 2: Update Signature
#[unsafe(no_mangle)]
pub fn show_completed_tasks(_ptr: i32, _len: i32) -> i64 {
    let db = DB.lock().unwrap();
    let completed: Vec<Task> = db.values().filter(|t| t.completed).cloned().collect();

    let serialized = bincode::serialize(&completed).unwrap();
    let ptr = serialized.as_ptr() as i32;
    let len = serialized.len() as i32;
    std::mem::forget(serialized);

    pack(ptr, len)
}

// These don't return i64, so they don't use the pack helper, but they work fine via direct call
#[unsafe(no_mangle)]
pub fn mark_as_completed(task_id: i32) {
    let mut db = DB.lock().unwrap();
    if let Some(task) = db.get_mut(&task_id) {
        task.completed = true;
    }
}

#[unsafe(no_mangle)]
pub fn change_priority(task_id: i32, new_priority: i32) {
    let mut db = DB.lock().unwrap();
    if let Some(task) = db.get_mut(&task_id) {
        task.priority = new_priority;
    }
}

#[unsafe(no_mangle)]
pub fn change_title(task_id: i32, title_ptr: i32, title_len: i32) {
    let title = unsafe {
        let slice = std::slice::from_raw_parts(title_ptr as *const u8, title_len as usize);
        String::from_utf8_lossy(slice).to_string()
    };

    let mut db = DB.lock().unwrap();
    if let Some(task) = db.get_mut(&task_id) {
        task.title = title;
    }
}

// FIX 2: Update Signature (just in case you call it generically later)
#[unsafe(no_mangle)]
pub fn query_by_id(task_id: i32, _unused: i32) -> i64 {
    let db = DB.lock().unwrap();

    let result = match db.get(&task_id) {
        Some(task) => QueryByIdResult::Success((*task).clone()),
        None => QueryByIdResult::NotFoundError,
    };

    let serialized = bincode::serialize(&result).unwrap();
    let ptr = serialized.as_ptr() as i32;
    let len = serialized.len() as i32;
    std::mem::forget(serialized);

    pack(ptr, len)
}
