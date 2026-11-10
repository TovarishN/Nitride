#![no_main]

#[path = "../support/incremental.rs"]
mod incremental;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    incremental::exercise(data);
});
