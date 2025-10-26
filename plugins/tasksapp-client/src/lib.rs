use serde::{Serialize, Deserialize};
use tasksapp_net::{Task, NewTaskResult};

#[link(wasm_import_module = "env")]
extern "C" {
    static mut memory: [u8; 0];
}

#[no_mangle]
pub extern "C" fn new_task(task_title_ptr: i32, task_title_len:, i32, priority_ptr: i32) -> (i32, i32)  {
    let task 
}
