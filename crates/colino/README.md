# colino

Converts triangle meshes into narrow-band signed distance fields on brodo's sparse volumes,
and reports how the conversion may have changed the shape's topology. It reports; it does not repair a mesh or refine a grid.

- **Reading**: binary and ASCII STL, Wavefront OBJ and 3MF, each under the limits the
  caller sets on triangles, vertices and bytes. Badly oriented, holed and degenerate
  meshes are read as they are. Each reader is a feature, `stl`, `obj` and `threemf`, all
  on by default; a user who reads no 3MF can leave out its ZIP and XML parsing.
- **Converting**: exact distances to the nearest triangle, signed by the generalised winding
  number (exact, or Barill et al.'s fast approximation), or, for a mesh whose triangles
  are turned at random, by a parity count along the axes that does not depend on their
  orientation (`Sign::Parity`), clamped to a band of ±N voxels.
  Leaves lie only in the band; inside regions become tiles; memory follows the band's
  volume, not the grid's.
- **Reporting**: the solid's and its complement's Betti numbers, the Euler characteristic
  against the input's when the input is a closed 2-manifold, and which named points share a
  component of the space around the solid. With the feature `serde`, the report serialises.
- **Hermite data**: on the corners of a conversion's voxels, signs and, on each edge whose
  ends differ, where the mesh crosses it, its normal and its triangle's group, in brodo's
  format (`colino::hermite`).

STEP is not read: a user working with B-reps triangulates with its own kernel and passes a
`Mesh`, whose `groups` and `group_names` carry the faces' names into the Hermite data.

```rust
use colino::convert::{Options, convert};
use colino::mesh::Limits;
use colino::report::{Place, report};

// An octahedron, its faces turned outward, as OBJ text.
let obj = b"v 10 0 0\nv -10 0 0\nv 0 10 0\nv 0 -10 0\nv 0 0 10\nv 0 0 -10\n\
            f 1 3 5\nf 3 2 5\nf 2 4 5\nf 4 1 5\nf 3 1 6\nf 2 3 6\nf 4 2 6\nf 1 4 6\n";
let limits = Limits::new(1 << 20, 1 << 20, 1 << 26);
let mesh = colino::obj::read(obj, limits)?;

// Voxels of 0.5 in the mesh's unit; OBJ states none, so the grid is told its own.
let options = Options::new(0.5).with_unit(Some("millimeter".into()));
let converted = convert(&mesh, &options)?;

let points = [("centre".to_string(), [0.1, 0.2, 0.3]), ("far".to_string(), [30.0, 0.0, 0.0])];
let r = report(&mesh, &converted, &points)?;
assert_eq!((r.solid.components, r.solid.tunnels, r.solid.cavities), (1, 0, 0));
assert_eq!(r.euler_change, Some(0));
assert_eq!(r.regions[0].place, Place::Solid);
# Ok::<(), Box<dyn std::error::Error>>(())
```

The examples convert a torus, checked against its exact distance, and a vented box whose
vent closes at a coarser voxel size:

```bash
cargo run -p colino --example torus
cargo run -p colino --example vent
cargo run -p colino --example torus -- part.stl 0.5
```

Each writes a slice as PNG and its report to `out/examples/colino/`; the last converts a
file of your own.

Licensed under either of MIT or Apache-2.0, at your option.
