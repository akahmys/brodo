//! Fuzzes `archive::from_bytes`, `archive::region_from_bytes` and `archive::list_from_bytes`,
//! the readers of archives (`crates/brodo/src/archive.rs`), and the reads of any file or
//! region they accept, every grid as every value type: no input may panic (TESTING.md T6),
//! and a file read whole is listed as it was read. The limit keeps what an archive may
//! decompress into within the fuzzer's memory.

#![no_main]

use brodo::archive::{Region, from_bytes, list_from_bytes, region_from_bytes};
use brodo::cache::{CacheFile, GridEntry};
use brodo::field::Stored;
use brodo::meta::Bounds;
use brodo::topology::Topology;
use libfuzzer_sys::fuzz_target;

#[path = "shared/extent.rs"]
mod extent;
#[path = "shared/probes.rs"]
mod probes;
use extent::extent_of;
use probes::PROBES;

const LIMIT: u64 = 1 << 26;

/// Reads, thaws and compares grid `g` of `file` as a grid of `T`, if it is one: the probes, its
/// leaves and one beyond them, its values on its own topology and on an empty one, and
/// whether every grid of `T` holds its tree.
fn whole<T: Stored>(file: &CacheFile, g: GridEntry<'_>) {
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

/// Reads `region` as a grid of `T`, if it is one: the probes, each leaf it holds and its
/// first voxel, and a leaf it does not.
fn region<T: Stored>(region: &Region) {
    let Ok(view) = region.view::<T>() else { return };
    for v in PROBES {
        let _read = view.get(v);
    }
    for (n, o) in view.leaves() {
        let _leaf = view.leaf(n);
        let _value = view.get(o);
    }
    let _beyond = view.leaf(u32::MAX);
}

/// Fails unless `file`, read whole, is listed as it was read, and its grids found by name
/// as the listing finds them.
fn listed_as_read(data: &[u8], file: &CacheFile) {
    let listing = list_from_bytes(data).expect("a file read whole is listed");
    let (listed, read) = (listing.grids(), file.grids());
    assert_eq!(listed.len(), read.len());
    for (l, g) in listed.iter().zip(&read) {
        assert_eq!((l.index(), l.meta(), l.bounds()), (g.index(), g.meta(), g.bounds()));
        assert_eq!((l.value_type(), l.encoding()), (g.value_type(), g.encoding()));
        let named: Vec<usize> =
            listing.grids_named(&l.meta().name).iter().map(|n| n.index()).collect();
        let found: Vec<usize> =
            file.grids_named(&g.meta().name).iter().map(|n| n.index()).collect();
        assert_eq!(named, found);
    }
    let (listed, read) = (listing.bytes_entries(), file.bytes_entries());
    assert_eq!(listed.len(), read.len());
    for (l, b) in listed.iter().zip(&read) {
        assert_eq!((l.name.as_str(), &l.extra), (b.name, b.extra));
        assert_eq!(usize::try_from(l.len).ok(), Some(b.bytes.len()));
    }
}

fuzz_target!(|data: &[u8]| {
    if let Ok(listing) = list_from_bytes(data) {
        for g in listing.grids() {
            let _named = listing.grids_named(&g.meta().name);
        }
    }
    if let Ok(file) = from_bytes(data, LIMIT) {
        listed_as_read(data, &file);
        for g in file.grids() {
            let _meta = g.meta();
            let _bounds = g.bounds();
            let _type = g.value_type();
            let _size = g.thaw_size();
            // Every value type docs/formats.md §1.1 allows.
            whole::<f32>(&file, g);
            whole::<f64>(&file, g);
            whole::<u8>(&file, g);
            whole::<u32>(&file, g);
            whole::<[f32; 3]>(&file, g);
            whole::<[f64; 3]>(&file, g);
        }
    }
    // Every grid the header states, as many as a directory entry a block each could hold,
    // and one beyond (docs/formats.md §8).
    let stated = data.get(8..12).map_or(0, |w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]));
    let grids = usize::try_from(stated).unwrap_or(usize::MAX).min(data.len() / 32);
    // A fixed box about the origin, and one the input's tail chooses.
    let extents = [Some(Bounds::Box { min: [-64; 3], max: [64; 3] }), extent_of(data)];
    for (i, extent) in (0..=grids).flat_map(|i| extents.iter().flatten().map(move |e| (i, *e))) {
        if let Ok(r) = region_from_bytes(data, i, extent, LIMIT) {
            region::<f32>(&r);
            region::<f64>(&r);
            region::<u8>(&r);
            region::<u32>(&r);
            region::<[f32; 3]>(&r);
            region::<[f64; 3]>(&r);
        }
    }
});
