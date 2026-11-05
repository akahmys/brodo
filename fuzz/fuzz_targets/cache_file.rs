//! Fuzzes `cache::from_bytes`, the reader of cache files (`crates/brodo/src/cache.rs`),
//! and the reads of any file it accepts, every grid as every value type: no input may panic
//! (TESTING.md T6).

#![no_main]

use brodo::cache::{CacheFile, GridEntry};
use brodo::field::Stored;
use brodo::topology::Topology;
use libfuzzer_sys::fuzz_target;

#[path = "shared/probes.rs"]
mod probes;
use probes::PROBES;

/// Reads, thaws and compares grid `g` of `file` as a grid of `T`, if it is one: a voxel, its
/// leaves and one beyond them, its values on its own topology and on an empty one, and
/// whether every grid of `T` holds its tree.
fn exercise<T: Stored>(file: &CacheFile, g: GridEntry<'_>) {
    let Ok(view) = g.view::<T>() else { return };
    for v in PROBES {
        let _read = view.get(v);
    }
    let count = u32::try_from(view.leaf_count()).unwrap_or(u32::MAX);
    for n in [0, count.saturating_sub(1), count] {
        let _leaf = view.leaf(n);
    }
    if let Ok((t, _)) = view.thaw() {
        let _again = view.thaw_on(&t);
    }
    let _on_empty = view.thaw_on(&Topology::new());
    for other in file.grids() {
        if let Ok(other) = other.view::<T>() {
            let _same = view.same_topology(&other);
        }
    }
}

fuzz_target!(|data: &[u8]| {
    if let Ok(file) = brodo::cache::from_bytes(data) {
        for g in file.grids() {
            let _meta = g.meta();
            let _bounds = g.bounds();
            let _type = g.value_type();
            let _size = g.thaw_size();
            // Every value type docs/formats.md §1.1 allows.
            exercise::<f32>(&file, g);
            exercise::<f64>(&file, g);
            exercise::<u8>(&file, g);
            exercise::<u32>(&file, g);
            exercise::<[f32; 3]>(&file, g);
            exercise::<[f64; 3]>(&file, g);
        }
    }
});
