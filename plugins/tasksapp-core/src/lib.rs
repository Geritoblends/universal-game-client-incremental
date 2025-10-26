use tasksapp_net::{Task, TaskResult};

static DB: Lazy<RefCell<HashMap<i32, Task>>> = Lazy::new(|| RefCell::new(HashMap::new()));

#[no_mangle]
pub extern "C" fn new_task(title_ptr: i32, title_len: i32, priority: i32) -> (i32, i32) {
    unimplemented!();
}

#[no_mangle]
pub extern "C" fn show_pending_tasks() -> (i32, i32) {
    unimplemented!();
}

#[no_mangle]
pub extern "C" fn show_completed_tasks() -> (i32, i32) {
    unimplemented!();
}

#[no_mangle]
pub extern "C" fn mark_as_completed(task_id: i32) {
    unimplemented!();
}

#[no_mangle]
pub extern "C" fn change_priority(task_id: i32) {
    unimplemented!();
}

#[no_mangle]
pub extern "C" fn change_title(task_id: i32) {
    unimplemented!();
}

#[no_mangle]
pub extern "C" fn query_by_id(task_id: i32) -> (i32, i32) {
    unimplemented!();
}

