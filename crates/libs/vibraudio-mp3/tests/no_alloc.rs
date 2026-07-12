use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

// Custom allocator that wraps System but counts every allocation
struct CountingAllocator;

static ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::SeqCst);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

// Replace the global allocator for this entire test binary
#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

#[test]
fn hot_path_zero_allocations() {
    use vibraudio_mp3::decoder::Mp3Decoder;
    use vibraudio_mp3::ffi::MINIMP3_MAX_SAMPLES_PER_FRAME;

    // Embed the test MP3 at compile time (this allocation happens at load)
    let mp3_data = include_bytes!("../test_data/silence.mp3");

    let mut decoder = Mp3Decoder::new();
    let mut pcm_buffer = [0i16; MINIMP3_MAX_SAMPLES_PER_FRAME];

    // Warm up: decode one frame to get past any initialization
    let mut offset: usize = 0;
    while offset < mp3_data.len() {
        match decoder.decode_frame(&mp3_data[offset..], &mut pcm_buffer) {
            Ok(result) => {
                offset += result.frame_bytes;
                break;
            }
            Err(_) => break,
        }
    }

    // Reset the counter to zero before measuring the hot path
    ALLOC_COUNT.store(0, Ordering::SeqCst);

    // Run 100 decode cycles on the hot path
    let mut hot_offset = offset;
    let mut cycles = 0;
    while hot_offset < mp3_data.len() && cycles < 100 {
        match decoder.decode_frame(&mp3_data[hot_offset..], &mut pcm_buffer) {
            Ok(result) => {
                hot_offset += result.frame_bytes;
                cycles += 1;
            }
            Err(_) => break,
        }
    }

    // Assert exactly zero heap allocations occurred
    let allocations = ALLOC_COUNT.load(Ordering::SeqCst);
    assert_eq!(
        allocations, 0,
        "Expected 0 allocations on hot path, got {}",
        allocations
    );
}
