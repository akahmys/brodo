//! Fuzzes `archive::from_bytes` and `archive::region_from_bytes`, the readers of archives
//! (`crates/brodo/src/archive.rs`), and the reads of any file or region they accept: no
//! input may panic (TESTING.md T6). The limit keeps what an archive may decompress into
//! within the fuzzer's memory.

#![no_main]

use brodo::archive::{from_bytes, region_from_bytes};
use brodo::meta::Bounds;
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
    let extent = Bounds::Box { min: [-64; 3], max: [64; 3] };
    if let Ok(region) = region_from_bytes(data, 1 << 26, 0, extent) {
        if let Ok(view) = region.view::<f32>() {
            let _read = view.get([0, 0, 0]);
            for (n, o) in view.leaves() {
                let _leaf = view.leaf(n);
                let _value = view.get(o);
            }
        }
    }
});
