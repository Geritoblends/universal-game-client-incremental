use std::collections::HashMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;

use tasksapp_net::{Task, NewTaskRequest, NewTaskResult, NewTaskError, QueryByIdResult};

static DB: Lazy<Mutex<HashMap<i32, Task>>> = Lazy::new(|| Mutex::new(HashMap::new()));
static mut CURRENT_ID: i32 = 0;

#[unsafe(no_mangle)]
pub extern "C" fn new_task(payload_ptr: i32, payload_len: i32) -> (i32, i32) {
    let payload = unsafe {
        let slice = std::slice::from_raw_parts(payload_ptr as *const u8, payload_len as usize);
        slice.to_vec()
    };
    
    let request: NewTaskRequest = match bincode::deserialize(&payload) {
        Ok(req) => req,
        Err(_) => {
            let result = NewTaskResult::Error(NewTaskError::TaskAlreadyExists);
            let serialized = bincode::serialize(&result).unwrap();
            let ptr = serialized.as_ptr() as i32;
            let len = serialized.len() as i32;
            std::mem::forget(serialized);
            return (ptr, len);
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
    
    (ptr, len)
}

#[unsafe(no_mangle)]
pub extern "C" fn show_pending_tasks() -> (i32, i32) {
    let db = DB.lock().unwrap();
    let pending: Vec<Task> = db
        .values()
        .filter(|t| !t.completed)
        .cloned()
        .collect();
    
    let serialized = bincode::serialize(&pending).unwrap();
    let ptr = serialized.as_ptr() as i32;
    let len = serialized.len() as i32;
    std::mem::forget(serialized);
    
    (ptr, len)
}

#[unsafe(no_mangle)]
pub extern "C" fn show_completed_tasks() -> (i32, i32) {
    let db = DB.lock().unwrap();
    let completed: Vec<Task> = db
        .values()
        .filter(|t| t.completed)
        .cloned()
        .collect();
    
    let serialized = bincode::serialize(&completed).unwrap();
    let ptr = serialized.as_ptr() as i32;
    let len = serialized.len() as i32;
    std::mem::forget(serialized);
    
    (ptr, len)
}

#[unsafe(no_mangle)]
pub extern "C" fn mark_as_completed(task_id: i32) {
    let mut db = DB.lock().unwrap();
    if let Some(task) = db.get_mut(&task_id) {
        task.completed = true;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn change_priority(task_id: i32, new_priority: u8) {
    let mut db = DB.lock().unwrap();
    if let Some(task) = db.get_mut(&task_id) {
        task.priority = new_priority;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn change_title(task_id: i32, title_ptr: i32, title_len: i32) {
    let title = unsafe {
        let slice = std::slice::from_raw_parts(title_ptr as *const u8, title_len as usize);
        String::from_utf8_lossy(slice).to_string()
    };
    
    let mut db = DB.lock().unwrap();
    if let Some(task) = db.get_mut(&task_id) {
        task.title = title;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn query_by_id(task_id: i32) -> (i32, i32) {
    let db = DB.lock().unwrap();
    
    let result = match db.get(&task_id) {
        Some(task) => QueryByIdResult::Success(task.clone()),
        None => QueryByIdResult::NotFoundError,
    };
    
    let serialized = bincode::serialize(&result).unwrap();
    let ptr = serialized.as_ptr() as i32;
    let len = serialized.len() as i32;
    std::mem::forget(serialized);
    
    (ptr, len)
}

#[unsafe(no_mangle)]
pub extern "C" fn dealloc(ptr: i32, len: i32) {
    unsafe {
        let _ = Vec::from_raw_parts(ptr as *mut u8, len as usize, len as usize);
    }
}
