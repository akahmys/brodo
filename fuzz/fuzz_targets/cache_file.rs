//! Fuzzes `CacheFile::from_bytes`, the reader of cache files (`crates/brodo/src/cache.rs`),
//! and the reads of any file it accepts, every grid as every value type: no input may panic
//! (TESTING.md T6).

#![no_main]

use brodo::cache::CacheFile;
use brodo::field::Stored;
use brodo::topology::Topology;
use libfuzzer_sys::fuzz_target;

/// Reads, thaws and compares grid `i` of `file` as a grid of `T`, if it is one: a voxel, its
/// leaves and one beyond them, its values on its own topology and on an empty one, and
/// whether every grid of `T` holds its tree.
fn exercise<T: Stored>(file: &CacheFile, i: usize) {
    let Ok(view) = file.view::<T>(i) else { return };
    let _read = view.get([0, 0, 0]);
    let count = u32::try_from(view.leaf_count()).unwrap_or(u32::MAX);
    for n in [0, count.saturating_sub(1), count] {
        let _leaf = view.leaf(n);
    }
    if let Ok((t, _)) = view.thaw() {
        let _again = view.thaw_on(&t);
    }
    let _on_empty = view.thaw_on(&Topology::new());
    for j in 0..file.grid_count() {
        if let Ok(other) = file.view::<T>(j) {
            let _same = view.same_topology(&other);
        }
    }
}

fuzz_target!(|data: &[u8]| {
    if let Ok(file) = CacheFile::from_bytes(data) {
        for i in 0..file.grid_count() {
            let _meta = file.meta(i);
            let _bounds = file.bounds(i);
            let _type = file.value_type(i);
            // Every value type docs/formats.md §1.1 allows.
            exercise::<f32>(&file, i);
            exercise::<f64>(&file, i);
            exercise::<u8>(&file, i);
            exercise::<u32>(&file, i);
            exercise::<[f32; 3]>(&file, i);
            exercise::<[f64; 3]>(&file, i);
        }
    }
});
