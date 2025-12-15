use crate::host::caller_state::HostState;
use anyhow::Result;
use wasmtime::Caller;

pub fn host_print(caller: Caller<'_, HostState>, ptr: i32, len: i32) -> Result<()> {
    let mem = caller.data().shared_memory.data();
    if ptr < 0 || (ptr as usize + len as usize) > mem.len() {
        return Ok(());
    }

    let base_ptr = mem.as_ptr() as *const u8;

    let s = unsafe {
        std::str::from_utf8_unchecked(std::slice::from_raw_parts(
            base_ptr.add(ptr as usize),
            len as usize,
        ))
    };
    println!("{}", s);
    Ok(())
}
