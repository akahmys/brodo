//! Fuzzes `HermiteView::find`, the reader of Hermite data in a cache file
//! (`crates/brodo/src/hermite.rs`), for every set a file names and for one it does not:
//! no input may panic (TESTING.md T6).

#![no_main]

use brodo::cache::CacheFile;
use brodo::hermite::HermiteView;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(file) = CacheFile::from_bytes(data) {
        let mut sets = vec![String::new()];
        for i in 0..file.grid_count() {
            if let Some(set) = file.meta(i).ok().and_then(|m| m.extra.get("hermite.set").cloned()) {
                sets.push(set);
            }
        }
        for set in sets {
            if let Ok(view) = HermiteView::find(&file, &set) {
                let _read = view.signs().get([0, 0, 0]);
            }
        }
    }
});
