#[test]
#[cfg(target_os = "linux")]
fn ffi_links_correctly() {
    use std::ffi::CStr;
    use std::mem::MaybeUninit;

    use vibraudio_mp3::ffi::{mp3dec_init, Mp3Dec};

    // Verify minimp3 links: initialize a decoder on the stack
    let mut dec = unsafe { MaybeUninit::<Mp3Dec>::zeroed().assume_init() };
    unsafe { mp3dec_init(&mut dec) };

    // Verify libasound links: convert error code 0 to a string
    let msg = unsafe {
        let ptr = vibraudio_alsa::ffi::snd_strerror(0);
        CStr::from_ptr(ptr)
    };
    assert!(!msg.to_str().unwrap().is_empty());
}
