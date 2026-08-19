mod backend;
mod error;
pub mod ffi;

pub use backend::WasapiBackend;

#[cfg(target_os = "windows")]
#[no_mangle]
pub extern "system" fn DllMain(
    _hinst: *mut std::ffi::c_void,
    _reason: u32,
    _reserved: *mut std::ffi::c_void,
) -> u32 {
    unsafe {
        let _ = ffi::CoInitializeEx(std::ptr::null_mut(), 0x0);
    }
    1
}
