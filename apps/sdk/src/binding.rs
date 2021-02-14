use crate::time;

use super::{executor, send_log};

/* Added by the macro
#[no_mangle]
pub extern "C" fn __exocore_app_init() {}
*/

#[no_mangle]
pub extern "C" fn __exocore_init() {
    // TODO: init logging
    executor::create_executor();
}

#[no_mangle]
pub extern "C" fn __exocore_tick() -> u64 {
    time::poll_timers();
    executor::poll_executor();

    // return time at which we want to be tick
    time::next_timer_time().unwrap_or(0)
}

// TODO: Remove
#[no_mangle]
pub extern "C" fn send_resp(bytes: *const u8, len: usize) {
    unsafe {
        let bytes = std::slice::from_raw_parts(bytes, len);
        let string = String::from_utf8_lossy(bytes);
        send_log(string.as_ref());
    }
}

#[no_mangle]
pub unsafe fn __exocore_alloc(size: usize) -> *mut u8 {
    let align = std::mem::align_of::<usize>();
    let layout = std::alloc::Layout::from_size_align_unchecked(size, align);
    std::alloc::alloc(layout)
}

#[no_mangle]
pub unsafe fn __exocore_free(ptr: *mut u8, size: usize) {
    let align = std::mem::align_of::<usize>();
    let layout = std::alloc::Layout::from_size_align_unchecked(size, align);
    std::alloc::dealloc(ptr, layout);
}
