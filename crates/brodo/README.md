# brodo

Sparse volumes in three dimensions, in pure Rust. A grid stores values only where you
activate voxels, in leaves of 8³, with a single value (a tile) standing for a whole
region that is uniform. It is not tied to any one use: signed distance fields, fluid
fields and smoke are all fields on its tree.

What sets it apart is that the set of active voxels, the `Topology`, is its own value,
and any number of fields of any type sit on one topology. You edit the topology once, and
every field on it is carried over to the new one; a field refuses a topology it does not
belong to.

- **Tree and fields**: a fixed four-level tree, tiles at every level. Editing a
  `Topology` returns the new one and a `Remap`, which carries each `Field<T>` over.
- **Work on leaves**: value accessors, each leaf's 26 neighbours, a leaf gathered with
  ghost layers, and leaf-parallel updates that give the same result at any thread count.
- **Analysis**: connected components and flood fill, the Euler characteristic, and
  dilation and erosion, with 6- or 26-connectivity, tiles included.
- **Files**, whose bytes docs/formats.md specifies for a reader in any language:
  - `freeze` packs a grid into one aligned buffer, read in place, with no copy, as a
    `GridView`.
  - A cache file holds several grids with their names, voxel sizes and units. It is
    checked whole on reading, and read by copy or by memory mapping.
  - An archive file is a cache file compressed, a floating grid's values kept exact or
    quantised. It reads back whole, or as one grid's region, decompressing only the parts
    that region needs.
  - OpenVDB's `.vdb`: float and double grids, written for OpenVDB's tools and read from
    files brodo and OpenVDB 12 to 14 write.
- **Hermite data**: where a surface crosses each voxel edge, with its normal and a group
  naming the face, beside the signs at the voxels' corners: what a reader needs to place
  the surface within a voxel, sharp edges kept.
- **Checking output**: a slice drawn as PNG, and a field for ParaView, to look at a result.

Where to start:

| To | Use |
| :--- | :--- |
| make and edit the set of active voxels | `topology::Topology` and its `edit` |
| hold values on it | `field::Field` |
| label components, find the Euler characteristic, dilate or erode | `analysis`, `euler`, `morphology` |
| save and load | `cache`, `archive`, `vdb` |
| read a saved grid in place | `frozen::freeze`, `view::GridView` |
| look at a field | `slice`, `vtk` |

Features, each a part a user may leave out or opt into:

| Feature | Default | Gives |
| :--- | :--- | :--- |
| `mmap` | on | reading a cache file by memory mapping |
| `archive` | off | the archive profile |
| `vdb` | off | OpenVDB's `.vdb` files |
| `slice-png` | off | a slice as PNG |
| `vtk` | off | a field for ParaView, as `.vtu` |

## Example

A solid ball in two leaves: its components and Euler characteristic counted, and the grid
saved as a cache file's bytes and read back:

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

// Serialised as a cache file's bytes, and read back from a copy of them.
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

It is written by a human, its owner, together with AI coding agents, chiefly Claude Code
and Antigravity in support: the owner decides and approves, the agents write under the
repository's rules, and every change passes the same checks and tests (the repository's
README says more).

Issues are welcome at <https://github.com/akahmys/brodo/issues>, a bug with the steps
that show it or a request. Pull requests are not accepted: code written elsewhere cannot
be shown free of copyleft sources, so fixes are written here from issues.

Licensed under either of MIT or Apache-2.0, at your option.
