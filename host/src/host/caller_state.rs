use crate::allocator::HostHeap;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use wasmtime::{Instance, SharedMemory, Table};

#[derive(Clone)]
pub struct HostState {
    pub instances: HashMap<String, Instance>,
    pub tables: HashMap<String, Table>,
    pub shared_memory: SharedMemory,
    pub next_memory_offset: i32,
    pub next_stack_offset: i32,
    pub heap: Arc<Mutex<HostHeap>>,
    pub slot_size: i32,
    pub data_size: i32,
    pub heap_start_address: i32,
}
