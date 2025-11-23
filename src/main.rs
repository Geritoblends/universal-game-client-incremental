use wasmtime::*;

mod lib;
use lib::*;

fn unpack_i64_result(packed_i64: i64) -> (i32, i32) {
    // Pointer is in the lower 32 bits (Wasm convention)
    let ptr: i32 = (packed_i64 & 0xFFFFFFFF) as i32;
    // Length is in the upper 32 bits
    let len: i32 = (packed_i64 >> 32) as i32;
    (ptr, len)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (mut store, _instance_core, instance_client) = setup_runtime()?;

    println!("🚀 Modules loaded with shared memory!\n");

    // Get the memory export to write inputs
    let mem = instance_client
        .get_memory(&mut store, "memory")
        .expect("memory export");

    let title_ptr = 1000;

    let create_task =
        instance_client.get_typed_func::<(i32, i32, i32), i64>(&mut store, "create_task")?;

    let title = "Learn Wasmtime Shared Memory";
    let mem_data = mem.data_mut(&mut store);

    // Write the title to memory
    mem_data[title_ptr..title_ptr + title.len()].copy_from_slice(title.as_bytes());

    // Call returns i64
    let result_i64 = create_task.call(
        &mut store,
        (
            title_ptr as i32,   // title_ptr
            title.len() as i32, // title_len
            5,                  // priority
        ),
    )?;

    // Unpack the i64 result
    let (result_ptr, result_len) = unpack_i64_result(result_i64);

    println!("✅ Task created!");
    println!("    Result at ptr={}, len={}", result_ptr, result_len);

    // --- Test: List pending tasks ---
    // FIX: Signature must be i64 to match the Wasm ABI for returning a packed (i32, i32) tuple.
    let list_pending =
        instance_client.get_typed_func::<(), i64>(&mut store, "list_pending_tasks")?;

    // Call returns i64
    let tasks_i64 = list_pending.call(&mut store, ())?;

    // Unpack the i64 result
    let (tasks_ptr, tasks_len) = unpack_i64_result(tasks_i64);

    println!("\n✅ Pending tasks listed!");
    println!("    Data at ptr={}, len={}", tasks_ptr, tasks_len);

    println!("\n🎉 All tests passed!");

    Ok(())
}
