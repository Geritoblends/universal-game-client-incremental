use crate::host::caller_state::HostState;
use wasmtime::Caller;

const WASM_PAGE_SIZE: u64 = 65536;
const GROWTH_CHUNK_SIZE: u64 = 80;
const HEAP_START_ADDR: u32 = 32 * 1024 * 1024;

pub fn host_alloc(caller: Caller<'_, HostState>, size: i32) -> i32 {
    let size = (size as u32 + 7) & !7;
    let memory = caller.data().shared_memory.clone();
    let mut heap = caller.data().heap.lock().unwrap();

    if let Some(addr) = heap.alloc(size) {
        return addr as i32;
    }

    let current_mem_size = (memory.size() * WASM_PAGE_SIZE) as u64;
    let growth_start_addr =
        if heap.free_blocks.is_empty() && current_mem_size < HEAP_START_ADDR as u64 {
            HEAP_START_ADDR
        } else {
            current_mem_size as u32
        };

    let required_growth = std::cmp::max(
        GROWTH_CHUNK_SIZE,
        (size as u64 + WASM_PAGE_SIZE - 1) / WASM_PAGE_SIZE,
    );

    if memory.grow(required_growth).is_err() {
        return 0;
    }

    let new_block_size = (required_growth * WASM_PAGE_SIZE) as u32;
    heap.dealloc(growth_start_addr, new_block_size);

    heap.alloc(size).unwrap_or(0) as i32
}

pub fn host_dealloc(caller: Caller<'_, HostState>, ptr: i32, size: i32) {
    if ptr == 0 {
        return;
    }
    let ptr = ptr as u32;
    let size = (size as u32 + 7) & !7;
    caller.data().heap.lock().unwrap().dealloc(ptr, size);
}
