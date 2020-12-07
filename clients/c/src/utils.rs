use exocore_core::utils::id::{generate_id, generate_prefixed_id};
use std::ffi::{CStr, CString};

/// Generates a unique identifier, optionally prefixed by the given string.
///
/// # Safety
/// * `prefix` can be null OR a valid string.
/// * `prefix` is owned by caller.
/// * Returned string must be freed using `exocore_free_string`.
#[no_mangle]
pub unsafe extern "C" fn exocore_generate_id(prefix: *const libc::c_char) -> *mut libc::c_char {
    let generated = if prefix.is_null() {
        generate_id()
    } else {
        let prefix = CStr::from_ptr(prefix).to_string_lossy();
        generate_prefixed_id(&prefix)
    };

    CString::new(generated).unwrap().into_raw()
}

/// Frees a string returned by one of the method of this library.
///
/// # Safety
/// * `ptr` should be a valid string returned by one of the method of this library.
/// * This method shall only be called once per string.
#[no_mangle]
pub unsafe extern "C" fn exocore_free_string(ptr: *mut libc::c_char) {
    drop(CString::from_raw(ptr))
}
