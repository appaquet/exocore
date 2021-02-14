use crate::time;

use super::executor;

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
