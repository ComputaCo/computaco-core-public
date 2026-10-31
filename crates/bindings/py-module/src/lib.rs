//! Python as a module of its own (ops/engineering/app-modules.md § Python on first use): the
//! interpreter and its standard library, answering the engine's runs
//! (`cw_modules::script`). Everything a program touches (its files, the world's network,
//! child processes, the clock, entropy, a debugger) it asks the core for through the
//! import `cw.core`, channel 1.
//!
//! The exports are the application modules' (`cw_alloc`, `cw_call`, `cw_free`).
use cw_modules::script::{serve, Served};

/// The last reply's address and length, read by the host right after `cw_call`.
static mut REPLY: [u32; 2] = [0, 0];

const PYTHON: Served = Served {
    run: cw_pyvm::run,
    run_debug: cw_pyvm::run_debug,
};

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
    let reply = serve(&request, ask_core, &PYTHON).into_boxed_slice();
    drop(request);
    let len = reply.len() as u32;
    REPLY = [Box::into_raw(reply) as *mut u8 as u32, len];
    std::ptr::addr_of!(REPLY) as *const u32
}

/// One host call, asked of the core on channel 1.
#[cfg(target_arch = "wasm32")]
fn ask_core(payload: &[u8]) -> Vec<u8> {
    #[link(wasm_import_module = "cw")]
    extern "C" {
        fn core(ptr: *const u8, len: usize) -> *mut u8;
    }
    let mut request = Vec::with_capacity(payload.len() + 1);
    request.push(1);
    request.extend_from_slice(payload);
    // SAFETY: the host answers with a buffer from `cw_alloc`: a little-endian `u32`
    // length, then that many bytes; it is this module's to free.
    unsafe {
        let at = core(request.as_ptr(), request.len());
        let len = u32::from_le_bytes(*(at as *const [u8; 4])) as usize;
        let reply = std::slice::from_raw_parts(at.add(4), len).to_vec();
        cw_free(at, len + 4);
        reply
    }
}
/// Natively there is no core to ask: the module is only ever run as Wasm.
#[cfg(not(target_arch = "wasm32"))]
fn ask_core(_: &[u8]) -> Vec<u8> {
    Vec::new()
}
