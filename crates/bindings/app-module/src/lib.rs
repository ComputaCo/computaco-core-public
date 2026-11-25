//! One application module (the engine's design notes): the code of the applications one
//! feature names, answering the engine's calls (`cw_applications::apps::module`).
//!
//! The interface is three exports over the module's own memory: `cw_alloc(len)` gives a
//! buffer for a request, `cw_call(ptr, len)` answers it and returns the address of two
//! little-endian `u32`s (the reply's address and length), and `cw_free(ptr, len)`
//! returns the reply's buffer. One import, `cw.core(ptr, len) -> ptr`, asks the core
//! while a call runs (the metric tables and faces text is measured with): the host
//! passes the request to the engine's `core` and writes the answer into a buffer from
//! `cw_alloc`, its length as a little-endian `u32` first, which the module frees.
//!
//! The exports are written by [`exports!`], so a crate that carries applications of its own
//! (registered with `cw_applications::apps::external`) builds a module of them with the
//! same interface: it leaves this crate's `exports` feature off and writes
//! `cw_app_module::exports!(its_install_function);` with a `core-text` feature of its own
//! that turns on this crate's.
pub use cw_applications;
pub use cw_scene;

/// The module's three exports. `$install` runs once, before the first call is answered:
/// it registers the applications the module carries.
#[macro_export]
macro_rules! exports {
    ($install:path) => {
        /// The last reply's address and length, read by the host right after `cw_call`.
        static mut REPLY: [u32; 2] = [0, 0];
        static INSTALL: ::std::sync::Once = ::std::sync::Once::new();

        /// A buffer of `len` bytes for the host to write a request into.
        #[no_mangle]
        pub extern "C" fn cw_alloc(len: usize) -> *mut u8 {
            Box::into_raw(vec![0u8; len].into_boxed_slice()) as *mut u8
        }
        /// # Safety
        /// `ptr` and `len` are a buffer this module gave out (`cw_alloc` or a reply).
        #[no_mangle]
        pub unsafe extern "C" fn cw_free(ptr: *mut u8, len: usize) {
            drop(Box::from_raw(::std::ptr::slice_from_raw_parts_mut(ptr, len)));
        }
        /// # Safety
        /// `ptr` and `len` are a buffer from `cw_alloc` holding the request; it is freed here.
        #[no_mangle]
        pub unsafe extern "C" fn cw_call(ptr: *mut u8, len: usize) -> *const u32 {
            INSTALL.call_once($install);
            #[cfg(all(feature = "core-text", target_arch = "wasm32"))]
            $crate::cw_scene::tables::set_source(|request| ask_core(0, request));
            let request = Box::from_raw(::std::ptr::slice_from_raw_parts_mut(ptr, len));
            let reply = $crate::cw_applications::apps::module::guest::serve(&request).into_boxed_slice();
            drop(request);
            let len = reply.len() as u32;
            REPLY = [Box::into_raw(reply) as *mut u8 as u32, len];
            ::std::ptr::addr_of!(REPLY) as *const u32
        }

        /// One question to the core on `channel` (0: the text tables).
        #[cfg(all(feature = "core-text", target_arch = "wasm32"))]
        fn ask_core(channel: u8, payload: &[u8]) -> Vec<u8> {
            #[link(wasm_import_module = "cw")]
            extern "C" {
                fn core(ptr: *const u8, len: usize) -> *mut u8;
            }
            let mut request = Vec::with_capacity(payload.len() + 1);
            request.push(channel);
            request.extend_from_slice(payload);
            // SAFETY: the host answers with a buffer from `cw_alloc`: a little-endian `u32`
            // length, then that many bytes; it is this module's to free.
            unsafe {
                let at = core(request.as_ptr(), request.len());
                let len = u32::from_le_bytes(*(at as *const [u8; 4])) as usize;
                let reply = ::std::slice::from_raw_parts(at.add(4), len).to_vec();
                cw_free(at, len + 4);
                reply
            }
        }
    };
}

/// This crate's own module carries only what `cw-applications` carries.
#[cfg(feature = "exports")]
fn install() {}
#[cfg(feature = "exports")]
exports!(install);
