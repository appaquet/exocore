use std::ffi::CString;
// use wasm_bindgen::prelude::*;

#[link(wasm_import_module = "the-wasm-import-module")]
extern "C" {
    fn log(prefix: *const i8, len: usize);
}

#[no_mangle]
pub extern "C" fn print_hello() {
    let str = CString::new("hello world").unwrap();
    unsafe {
        log(str.as_ptr(), 11);
    }
}
