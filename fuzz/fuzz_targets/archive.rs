//! Fuzzes `archive::from_bytes` and `archive::region_from_bytes`, the readers of archives
//! (`crates/brodo/src/archive.rs`), and the reads of any file or region they accept, every
//! grid as every value type: no input may panic (TESTING.md T6). The limit keeps what an
//! archive may decompress into within the fuzzer's memory.

#![no_main]

use brodo::archive::{Region, from_bytes, region_from_bytes};
use brodo::cache::CacheFile;
use brodo::field::Stored;
use brodo::meta::Bounds;
use brodo::topology::Topology;
use libfuzzer_sys::fuzz_target;

const LIMIT: u64 = 1 << 26;

/// Reads, thaws and compares grid `i` of `file` as a grid of `T`, if it is one: a voxel, its
/// leaves and one beyond them, its values on its own topology and on an empty one, and
/// whether every grid of `T` holds its tree.
fn whole<T: Stored>(file: &CacheFile, i: usize) {
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

/// Reads `region` as a grid of `T`, if it is one: a voxel, each leaf it holds and its first
/// voxel, and a leaf it does not.
fn region<T: Stored>(region: &Region) {
    let Ok(view) = region.view::<T>() else { return };
    let _read = view.get([0, 0, 0]);
    for (n, o) in view.leaves() {
        let _leaf = view.leaf(n);
        let _value = view.get(o);
    }
    let _beyond = view.leaf(u32::MAX);
}

fuzz_target!(|data: &[u8]| {
    if let Ok(file) = from_bytes(data, LIMIT) {
        for i in 0..file.grid_count() {
            let _meta = file.meta(i);
            let _bounds = file.bounds(i);
            // Every value type docs/formats.md §1.1 allows.
            whole::<f32>(&file, i);
            whole::<f64>(&file, i);
            whole::<u8>(&file, i);
            whole::<u32>(&file, i);
            whole::<[f32; 3]>(&file, i);
            whole::<[f64; 3]>(&file, i);
        }
    }
    // Every grid the header states, as many as a directory entry a block each could hold,
    // and one beyond (docs/formats.md §8).
    let stated = data.get(8..12).map_or(0, |w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]));
    let grids = usize::try_from(stated).unwrap_or(usize::MAX).min(data.len() / 32);
    let extent = Bounds::Box { min: [-64; 3], max: [64; 3] };
    for i in 0..=grids {
        if let Ok(r) = region_from_bytes(data, LIMIT, i, extent) {
            region::<f32>(&r);
            region::<f64>(&r);
            region::<u8>(&r);
            region::<u32>(&r);
            region::<[f32; 3]>(&r);
            region::<[f64; 3]>(&r);
        }
    }
});
