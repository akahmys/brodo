//! Fuzzes `CacheFile::from_bytes`, the reader of cache files (`crates/brodo/src/cache.rs`),
//! and the reads of any file it accepts: no input may panic (TESTING.md T6).

#![no_main]

use brodo::cache::CacheFile;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(file) = CacheFile::from_bytes(data) {
        for i in 0..file.grid_count() {
            let _meta = file.meta(i);
            let _bounds = file.bounds(i);
            if let Ok(view) = file.view::<f32>(i) {
                let _read = view.get([0, 0, 0]);
            }
        }
    }
});
