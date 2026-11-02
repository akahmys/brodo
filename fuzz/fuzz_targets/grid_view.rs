//! Fuzzes `GridView::new`, the reader of frozen grids (`crates/brodo/src/view.rs`), and the
//! reads, thawing and comparison of any view it accepts, of every value type: no input may
//! panic (TESTING.md T6).

#![no_main]

use brodo::field::Stored;
use brodo::topology::Topology;
use brodo::view::GridView;
use libfuzzer_sys::fuzz_target;

#[path = "shared/probes.rs"]
mod probes;
use probes::PROBES;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

/// A 32-byte aligned block, so that the input reaches the reader aligned, as a file read
/// into memory would.
#[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Clone, Copy)]
#[repr(C, align(32))]
struct Block([u8; 32]);


/// Reads, thaws and compares any view of `bytes` as a grid of `T` accepts: its voxels, its
/// leaves and one beyond them, its values on its own topology and on an empty one.
fn exercise<T: Stored>(bytes: &[u8]) {
    let Ok(view) = GridView::<T>::new(bytes) else { return };
    for v in PROBES {
        let _read = view.get(v);
    }
    let count = u32::try_from(view.leaf_count()).unwrap_or(u32::MAX);
    for n in [0, count / 2, count.saturating_sub(1), count] {
        let _leaf = view.leaf(n);
    }
    if let Ok((t, _)) = view.thaw() {
        let _again = view.thaw_on(&t);
    }
    let _on_empty = view.thaw_on(&Topology::new());
    let _same = view.same_topology(&view);
}

fuzz_target!(|data: &[u8]| {
    let mut blocks = vec![Block([0; 32]); data.len().div_ceil(32)];
    blocks.as_mut_bytes()[..data.len()].copy_from_slice(data);
    let bytes = &blocks.as_bytes()[..data.len()];
    // Every value type docs/formats.md §1.1 allows.
    exercise::<f32>(bytes);
    exercise::<f64>(bytes);
    exercise::<u8>(bytes);
    exercise::<u32>(bytes);
    exercise::<[f32; 3]>(bytes);
    exercise::<[f64; 3]>(bytes);
});
