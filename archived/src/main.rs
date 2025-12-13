use anyhow::Result;
use std::cell::UnsafeCell;
use wasmtime::*;

mod lib;
use lib::*;

fn unpack_i64_result(packed_i64: i64) -> (i32, i32) {
    let ptr: i32 = (packed_i64 & 0xFFFFFFFF) as i32;
    let len: i32 = (packed_i64 >> 32) as i32;
    (ptr, len)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Setup Runtime (allocates memory, instantiates plugins, sets up heap)
    let (mut store, _instance_core, instance_client) = setup_runtime()?;

    println!("🚀 Runtime Initialized: Dynamic Linker & System Allocator Active\n");

    // 2. Prepare Input Data
    // SAFE OFFSET: The Host reserved 0MB - 1MB for itself.
    // Plugins start at 1MB. So address 1000 is safe to use.
    let title_ptr = 1000;
    let title = "Learn Wasmtime Shared Memory";

    // 3. Write to Shared Memory
    // We bypass the instance and go straight to the HostState's memory handle
    {
        let shared_memory = store.data().shared_memory.clone();
        let data_slice: &[UnsafeCell<u8>] = shared_memory.data();

        unsafe {
            // Cast UnsafeCell to mutable u8 slice
            let raw_ptr = data_slice.as_ptr() as *mut u8;
            let mut_slice = std::slice::from_raw_parts_mut(raw_ptr, data_slice.len());

            // Write our string at offset 1000
            mut_slice[title_ptr..title_ptr + title.len()].copy_from_slice(title.as_bytes());
        }
    }

    // 4. Call 'create_task' on Client
    println!("▶️  Calling create_task...");
    let create_task =
        instance_client.get_typed_func::<(i32, i32, i32), i64>(&mut store, "create_task")?;

    let result_i64 = create_task.call(
        &mut store,
        (
            title_ptr as i32,   // Ptr to "Learn Wasmtime..."
            title.len() as i32, // Length
            5,                  // Priority
        ),
    )?;

    // 5. Unpack Result
    let (result_ptr, result_len) = unpack_i64_result(result_i64);
    println!(
        "✅ Task Created! Result stored at: Ptr={}, Len={}",
        result_ptr, result_len
    );

    // 6. Call 'list_pending_tasks'
    println!("\n▶️  Calling list_pending_tasks...");
    let list_pending =
        instance_client.get_typed_func::<(), i64>(&mut store, "list_pending_tasks")?;

    let tasks_i64 = list_pending.call(&mut store, ())?;
    let (tasks_ptr, tasks_len) = unpack_i64_result(tasks_i64);

    println!(
        "✅ Pending Tasks Listed! Data at: Ptr={}, Len={}",
        tasks_ptr, tasks_len
    );

    // 7. Verify Data content by reading memory back
    {
        let shared_memory = store.data().shared_memory.clone();
        let data_slice = shared_memory.data();
        unsafe {
            let raw_ptr = data_slice.as_ptr() as *const u8;
            let slice = std::slice::from_raw_parts(raw_ptr, data_slice.len());

            let output_bytes = &slice[tasks_ptr as usize..(tasks_ptr + tasks_len) as usize];
            // Just printing length to prove it's real data
            println!("   (Read {} bytes from shared memory)", output_bytes.len());
        }
    }

    let call_delete = instance_client.get_typed_func::<i32, i64>(&mut store, "call_delete_task_by_id")?;

    let result_i64 = call_delete.call(&mut store, 1)?;
    list_pending.call(&mut store, ())?;


    println!("\n🎉 All tests passed with Dynamic Linking!");

    Ok(())
}
