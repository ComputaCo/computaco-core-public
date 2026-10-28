//! One application module (docs/app-modules.md): the code of the applications one
//! feature names, answering the engine's calls (`cw_applications::apps::module`).
//!
//! The interface is three exports over the module's own memory, so a host loads it
//! with nothing but `WebAssembly.instantiate` and no imports: `cw_alloc(len)` gives a
//! buffer for a request, `cw_call(ptr, len)` answers it and returns the address of
//! two little-endian `u32`s (the reply's address and length), and `cw_free(ptr, len)`
//! returns the reply's buffer.
/// The last reply's address and length, read by the host right after `cw_call`.
static mut REPLY: [u32; 2] = [0, 0];

/// A buffer of `len` bytes for the host to write a request into.
#[no_mangle]
pub extern "C" fn cw_alloc(len: usize) -> *mut u8 {
    Box::into_raw(vec![0u8; len].into_boxed_slice()) as *mut u8
}
/// # Safety
/// `ptr` and `len` are a buffer this module gave out (`cw_alloc` or a reply).
#[no_mangle]
pub unsafe extern "C" fn cw_free(ptr: *mut u8, len: usize) {
    drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)));
}
/// # Safety
/// `ptr` and `len` are a buffer from `cw_alloc` holding the request; it is freed here.
#[no_mangle]
pub unsafe extern "C" fn cw_call(ptr: *mut u8, len: usize) -> *const u32 {
    let request = Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len));
    let reply = cw_applications::apps::module::guest::serve(&request).into_boxed_slice();
    drop(request);
    let len = reply.len() as u32;
    REPLY = [Box::into_raw(reply) as *mut u8 as u32, len];
    std::ptr::addr_of!(REPLY) as *const u32
}
