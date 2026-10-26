//! Fuzzes `GridView::new`, the reader of frozen grids (`crates/brodo/src/view.rs`), and the
//! reads and thawing of any view it accepts: no input may panic (TESTING.md T6).

#![no_main]

use brodo::view::GridView;
use libfuzzer_sys::fuzz_target;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

/// A 32-byte aligned block, so that the input reaches the reader aligned, as a file read
/// into memory would.
#[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Clone, Copy)]
#[repr(C, align(32))]
struct Block([u8; 32]);

/// Voxels around every level's boundaries, read from any view accepted.
const PROBES: [[i32; 3]; 8] = [
    [0, 0, 0],
    [-1, -1, -1],
    [7, 8, 9],
    [127, 128, 129],
    [4095, 0, 4096],
    [-4097, 5, 300],
    [i32::MAX, 0, i32::MIN],
    [64, 64, 64],
];

fuzz_target!(|data: &[u8]| {
    let mut blocks = vec![Block([0; 32]); data.len().div_ceil(32)];
    blocks.as_mut_bytes()[..data.len()].copy_from_slice(data);
    let bytes = &blocks.as_bytes()[..data.len()];
    if let Ok(view) = GridView::<f32>::new(bytes) {
        for v in PROBES {
            let _read = view.get(v);
        }
        let _thawed = view.thaw();
    }
    if let Ok(view) = GridView::<[f32; 3]>::new(bytes) {
        for v in PROBES {
            let _read = view.get(v);
        }
        let _thawed = view.thaw();
    }
});
