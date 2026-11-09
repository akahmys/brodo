//! Fuzzes `HermiteView::find`, the reader of Hermite data in a cache file
//! (`crates/brodo/src/hermite.rs`), for every set a file names and for one it does not, and
//! the reads of each set it finds, its signs and the crossings of its first and last leaves'
//! first and last points on every axis; the sets are named by its grids and its entries of
//! bytes, by this version's keys or an earlier one's: no input may panic (TESTING.md T6).

#![no_main]

use brodo::field::Stored;
use brodo::hermite::HermiteView;
use brodo::view::GridView;
use libfuzzer_sys::fuzz_target;

/// Reads and thaws one grid of a set: a voxel, its first and last leaves and one beyond.
fn exercise<T: Stored>(view: &GridView<'_, T>) {
    let _read = view.get([0, 0, 0]);
    let count = u32::try_from(view.leaf_count()).unwrap_or(u32::MAX);
    for n in [0, count.saturating_sub(1), count] {
        let _leaf = view.leaf(n);
    }
    let _thawed = view.thaw();
}

fuzz_target!(|data: &[u8]| {
    if let Ok(file) = brodo::cache::from_bytes(data) {
        let mut sets = vec![String::new()];
        let grids = file.grids();
        let bytes_entries = file.bytes_entries();
        let extras = grids.iter().map(|g| &g.meta().extra);
        for extra in extras.chain(bytes_entries.iter().map(|b| b.extra)) {
            let set = extra.get("brodo.hermite.set").or_else(|| extra.get("hermite.set"));
            if let Some(set) = set {
                sets.push(set.clone());
            }
        }
        for set in sets {
            if let Ok(view) = HermiteView::find(&file, &set) {
                exercise(view.signs());
                let count = u32::try_from(view.signs().leaf_count()).unwrap_or(u32::MAX);
                for n in [0, count.saturating_sub(1), count] {
                    for axis in brodo::tree::Axis::ALL {
                        for place in [0, 511, 512] {
                            let _crossing = view.crossing(n, axis, place);
                        }
                    }
                }
                let _names = view.group_names().len();
            }
        }
    }
});
