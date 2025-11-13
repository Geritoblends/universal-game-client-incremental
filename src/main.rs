use wasmtime::*;

mod lib;
use lib::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (mut store, _instance_core, instance_client) = setup_runtime()?;
    
    println!("🚀 Modules loaded with shared memory!\n");
    
    // Test: Create a task
    let create_task = instance_client
        .get_typed_func::<(i32, i32, u8), (i32, i32)>(&mut store, "create_task")?;
    
    let title = "Learn Wasmtime Shared Memory";
    let mem = instance_client.get_memory(&mut store, "memory").expect("memory export");
    
    let mem_data = mem.data_mut(&mut store);
    let title_ptr = 1000;
    mem_data[title_ptr..title_ptr + title.len()].copy_from_slice(title.as_bytes());
    
    let (result_ptr, result_len) = create_task.call(
        &mut store,
        (title_ptr as i32, title.len() as i32, 5),
    )?;
    
    println!("✅ Task created!");
    println!("   Result at ptr={}, len={}", result_ptr, result_len);
    
    // Test: List pending tasks
    let list_pending = instance_client
        .get_typed_func::<(), (i32, i32)>(&mut store, "list_pending_tasks")?;
    
    let (tasks_ptr, tasks_len) = list_pending.call(&mut store, ())?;
    println!("\n✅ Pending tasks listed!");
    println!("   Data at ptr={}, len={}", tasks_ptr, tasks_len);
    
    println!("\n🎉 All tests passed!");
    
    Ok(())
}
