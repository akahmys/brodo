//! Fuzzes `archive::from_bytes`, the reader of archives (`crates/brodo/src/archive.rs`),
//! and the reads of any file it accepts: no input may panic (TESTING.md T6). The limit
//! keeps what an archive may decompress into within the fuzzer's memory.

#![no_main]

use brodo::archive::from_bytes;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(file) = from_bytes(data, 1 << 26) {
        for i in 0..file.grid_count() {
            let _meta = file.meta(i);
            let _bounds = file.bounds(i);
            if let Ok(view) = file.view::<f32>(i) {
                let _read = view.get([0, 0, 0]);
            }
        }
    }
});
