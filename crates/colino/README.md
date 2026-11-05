# colino

Converts triangle meshes into narrow-band signed distance fields on
[brodo](https://github.com/akahmys/brodo)'s sparse volumes, and reports how the
conversion may have changed the shape: a hole closed, a cavity opened, a wall too thin or
a gap too narrow for the voxel size. ARCHITECTURE.md §5 says what a conversion gives and
what each part of the report finds.

- **Reading**: binary and ASCII STL, Wavefront OBJ and 3MF, each into a `Mesh`, under
  the `Limits` you set on triangles, vertices and bytes, so that an untrusted file cannot
  exhaust memory. A mesh from elsewhere, a CAD kernel's triangulation say, is built as a
  `Mesh` directly; its `groups` and `group_names` carry the faces' names into the Hermite
  data.
- **Converting**: `convert` with `Options`, chiefly the voxel size in the mesh's unit and
  the band's half-width in voxels (`with_band`). It gives the topology, the distances
  (negative inside, in the mesh's unit) and the grid's metadata. Inside and outside are
  told by the generalised winding number, which holds where parts pass into each other or
  walls are doubled, approximate by default or exact (`with_accuracy`); or, for a mesh
  whose triangles face either way at random, by counting crossings
  (`with_sign(Sign::Parity)`).
- **Reporting**: `report` counts the components, tunnels and cavities of the solid and of
  the space around it; compares the Euler characteristic with the input's when the
  input is closed and manifold; says which component each point you name lies in; and
  lists the places thinner, or gaps narrower, than two voxels.
- **Hermite data**: where the mesh crosses each voxel edge, with its normal and the
  triangle's group, beside the signs at the corners, in brodo's format
  (`colino::hermite`).

Features, each a part a user may leave out or opt into:

| Feature | Default | Gives |
| :--- | :--- | :--- |
| `stl` | on | reading STL |
| `obj` | on | reading OBJ |
| `threemf` | on | reading 3MF, with its ZIP and XML parsing |
| `serde` | off | the report serialisable with serde |

## Example

An octahedron read from OBJ text, converted, and its report checked:

```rust
use colino::convert::{Options, convert};
use colino::mesh::Limits;
use colino::report::{Place, report};

// An octahedron, its faces turned outward, as OBJ text.
let obj = b"v 10 0 0\nv -10 0 0\nv 0 10 0\nv 0 -10 0\nv 0 0 10\nv 0 0 -10\n\
            f 1 3 5\nf 3 2 5\nf 2 4 5\nf 4 1 5\nf 3 1 6\nf 2 3 6\nf 4 2 6\nf 1 4 6\n";
// At most 2²⁰ triangles, 2²⁰ vertices and 64 MiB read.
let limits = Limits::new(1 << 20, 1 << 20, 1 << 26);
let mesh = colino::obj::from_bytes(obj, limits)?;

// Voxels of 0.5 in the mesh's unit; OBJ states none, so the grid is told its own.
let options = Options::new(0.5).with_unit(Some("millimeter".into()));
let converted = convert(&mesh, &options)?;

let points = [("centre".to_string(), [0.1, 0.2, 0.3]), ("far".to_string(), [30.0, 0.0, 0.0])];
// One solid piece with no tunnel or cavity, its Euler characteristic as the mesh's, and
// the first point inside it.
let r = report(&mesh, &converted, &points)?;
assert_eq!((r.solid.components, r.solid.tunnels, r.solid.cavities), (1, 0, 0));
assert_eq!(r.euler_change, Some(0));
assert_eq!(r.regions[0].place, Place::Solid);
# Ok::<(), Box<dyn std::error::Error>>(())
```

The examples convert a torus, checked against its exact distance, and a vented box whose
vent closes at a coarser voxel size. Each writes a slice as PNG and its report to
`out/examples/colino/`:

```bash
cargo run -p colino --example torus
cargo run -p colino --example vent
```

Given a mesh file of your own and a voxel size in its unit, the torus example converts
that file instead:

```bash
cargo run -p colino --example torus -- part.stl 0.5
```

It is written by a human, its owner, together with AI coding agents, chiefly Claude Code
and Antigravity in support: the owner decides and approves, the agents write under the
repository's rules, and every change passes the same checks and tests (the repository's
README says more).

Issues are welcome at <https://github.com/akahmys/brodo/issues>, a bug with the steps
that show it or a request. Pull requests are not accepted: code written elsewhere cannot
be shown free of copyleft sources, so fixes are written here from issues.

Licensed under either of MIT or Apache-2.0, at your option.
