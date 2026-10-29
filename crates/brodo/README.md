# brodo

Sparse volumes in three dimensions, in pure Rust: a VDB-style sparse tree whose fields
share one topology, a zero-copy frozen view and file format, and topology analysis. It
knows nothing of its users; signed distance fields, fluid fields and smoke are all fields
on its tree.

- **Tree and fields**: a fixed four-level tree of 8³ leaves with tiles at every level. A
  `Topology` is edited through an editor whose commit returns the new topology and a
  `Remap`; each `Field<T>` on it is carried over by that remap, and a field refuses a
  topology it does not belong to.
- **Work on leaves**: each leaf's 26 neighbours, value accessors, leaf-parallel updates
  that give the same result at any thread count, and gathers of a leaf with ghost layers.
- **Analysis**: connected components and flood fill, the Euler characteristic, and
  dilation and erosion, with 6- or 26-connectivity, tiles included.
- **Files**: `freeze` packs a grid into one aligned buffer, read in place as a
  `GridView`; the cache profile stores several grids with their metadata, read by copy or
  by memory mapping (feature `mmap`, on by default), checked whole first, as
  docs/formats.md §3.4 states; the archive profile compresses them, each grid's leaf
  values exact or quantised to 16 or 8 bits, and is read back as the same cache file
  (feature `archive`, opted into).
- **Hermite data**: signs on a grid's voxel corners and, on each edge whose ends differ,
  where a surface crosses it, its normal and its group, as a format any producer writes
  and any reader checks.
- **Checking output**: a slice as PNG (feature `slice-png`) and a field for ParaView
  (feature `vtk`), both opted into.

```rust
use brodo::analysis::{Connectivity, label};
use brodo::cache::{CacheEntry, CacheFile, to_bytes};
use brodo::euler::euler;
use brodo::field::Field;
use brodo::meta::GridMeta;
use brodo::topology::Topology;

// Two leaves side by side, and a solid ball of radius 5 about (8, 4, 4).
let empty = Topology::new();
let mut edit = empty.edit();
edit.activate([0, 0, 0])?;
edit.activate([8, 0, 0])?;
let (t, _remap) = edit.commit()?;

let mut solid = Field::new(&t, 0_u8);
for n in t.leaf_numbers() {
    let voxels: Vec<_> = t.leaf_voxels(n)?.collect();
    for (value, v) in solid.leaf_mut(&t, n)?.iter_mut().zip(voxels) {
        let d2 = (v[0] - 8).pow(2) + (v[1] - 4).pow(2) + (v[2] - 4).pow(2);
        *value = u8::from(d2 <= 25);
    }
}
let inside = |x: u8| x == 1;
assert_eq!(label(&t, &solid, inside, Connectivity::TwentySix)?.count(), 1);
assert_eq!(euler(&t, &solid, inside, Connectivity::TwentySix)?, 1);

// Written to a cache file and read back in place.
let meta = GridMeta::new("ball", [0.1; 3], "millimeter")?;
let bytes = to_bytes(&[CacheEntry::new(&meta, &t, &solid)?])?;
let file = CacheFile::from_bytes(&bytes)?;
assert_eq!(file.meta(0)?.name, "ball");
assert_eq!(file.view::<u8>(0)?.get([8, 4, 4])?, 1);
# Ok::<(), Box<dyn std::error::Error>>(())
```

The examples diffuse several fields on one topology, checked against the analytic
solution, and build the union of two spheres' distance fields:

```bash
cargo run -p brodo --example diffusion
cargo run -p brodo --example spheres
```

Licensed under either of MIT or Apache-2.0, at your option.
