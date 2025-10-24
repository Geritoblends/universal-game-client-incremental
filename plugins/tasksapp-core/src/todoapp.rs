use std::sync::Mutex;
use dashmap::DashMap;

pub struct VarChar100([u8; 100]);

#[repr(u8)]
pub enum Priority {
    Low,
    Regular,
    Urgent
}

pub struct Task {
    title: VarChar100,
    id: u32,
    pending: bool,
    priority: Priority,
}


pub struct TodoApp {
    tasks: DashMap<u32, Task>,
    counter: Mutex<i32>
}
