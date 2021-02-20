use super::executor;
use crate::{app, time};

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "exocore")]
extern "C" {
    pub(crate) fn __exocore_host_log(level: u8, bytes: *const u8, len: usize);
    pub(crate) fn __exocore_host_now() -> u64;
    pub(crate) fn __exocore_host_out_message(msg_type: u32, bytes: *const u8, len: usize) -> u32;
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) unsafe fn __exocore_host_log(_level: u8, _bytes: *const u8, _len: usize) {
    panic!("Not implemented in outside of wasm environment");
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) unsafe fn __exocore_host_now() -> u64 {
    panic!("Not implemented in outside of wasm environment");
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) unsafe fn __exocore_host_out_message(
    msg_type: u32,
    bytes: *const u8,
    len: usize,
) -> u32 {
    panic!("Not implemented in outside of wasm environment");
}

/* Added by the macro
#[no_mangle]
pub extern "C" fn __exocore_app_init() {}
*/

#[no_mangle]
pub extern "C" fn __exocore_init() {
    crate::log::init().expect("Couldn't setup logging");
    executor::create_executor();
}

#[no_mangle]
pub extern "C" fn __exocore_tick() -> u64 {
    time::poll_timers();
    executor::poll_executor();

    // return time at which we want to be tick
    time::next_timer_time().unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn __exocore_app_boot() {
    app::boot_app();
}

#[no_mangle]
pub extern "C" fn __exocore_in_message(msg_type: u32, ptr: *mut u8, size: usize) -> u32 {
    0
}

#[no_mangle]
pub unsafe extern "C" fn __exocore_alloc(size: usize) -> *mut u8 {
    let align = std::mem::align_of::<usize>();
    let layout = std::alloc::Layout::from_size_align_unchecked(size, align);
    std::alloc::alloc(layout)
}

#[no_mangle]
pub unsafe extern "C" fn __exocore_free(ptr: *mut u8, size: usize) {
    let align = std::mem::align_of::<usize>();
    let layout = std::alloc::Layout::from_size_align_unchecked(size, align);
    std::alloc::dealloc(ptr, layout);
}
