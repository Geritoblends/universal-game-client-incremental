// Definir el host state
pub struct HostState {
    instances: HashMap<String, Instance>
}

// definir host import `call`
#[no_mangle]
pub extern "C" fn call(
    caller: mut Caller<'_, HostState>,
    instance_id_ptr: i32,
    instance_id_len: i32,
    func_name_ptr: i32,
    func_name_len: i32,
    payload_ptr: i32,
    payload_len: i32) -> (i32, i32) {
    unimplemented!();
}

// definir host import `send_to_server`
#[no_mangle]
pub extern "C" fn send_to_server(caller: mut Caller<'_, HostState>, message_ptr: i32, message_len: i32) {
    unimplemented!();
}

// definir host import `fire_and_forget`
#[no_mangle]
pub extern "C" fn fire_and_forget(caller: mut Caller<'_, HostState>, 
    instance_id_ptr: i32,
    instance_id_len_ i32,
    func_ptr: i32,
    func_len: i32,
    payload_ptr: i32,
    payload_len: i32,
) {
    unimplemented!();
}
