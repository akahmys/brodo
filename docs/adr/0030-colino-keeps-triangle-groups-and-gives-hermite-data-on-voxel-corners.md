# ADR-0030: colino keeps triangle groups and gives Hermite data on voxel corners

- **Status**: Amended
- **Date**: 2026-10-20

## Context

A user building cut cells on brodo's grid asked for two things (ROADMAP.md §4, R11). The
first was a group per triangle, read from the file. The second was Hermite data on a
lattice: for each lattice edge the surface crosses, where it crosses, the surface's normal
there, and the crossed triangle's group. A sampled SDF cannot hold the kink where two
groups meet. The user answered brodo's questions on 2026-10-20.

The code as it stood:

- `Mesh` held vertices, triangles and a unit, with public fields.
- The OBJ reader skipped `g` and `o`.
- The 3MF reader kept no object identity once items were placed.
- A cache file held grids only, each a `CacheEntry` of metadata, topology and field, so a
  table that is not a grid had no place in one.
- The value types were `f32`, `f64`, `u8`, `u32`, `[f32; 3]` and `[f64; 3]`, codes 1 to 6
  (ADR-0029).

Read for this record:

- Ju, Losasso, Schaefer and Warren (2002), *Dual Contouring of Hermite Data*, ACM TOG
  21(3) 339–346, for Hermite data as signs at lattice points plus, on each edge whose
  ends differ, an exact crossing and normal.
- Woop, Benthin and Wald (2013), *Watertight Ray/Triangle Intersection*, JCGT 2(1) 65–82,
  for an edge test that misses no shared edge or vertex of adjoining triangles.

## Decision

The human decided, on 2026-10-20:

**Groups**

- `Mesh` gains `groups`: either empty, meaning every triangle is in group 0, or one `u32`
  per triangle. It also gains `group_names`, a name per id. Group 0 is always named `""`
  (no group). A group is identified by its name, so ids are given in the order each name
  first appears. Any other length of `groups`, or an id with no name, is a `MeshError`.
- **OBJ**: a triangle takes the names of the last `g`, joined by one space. Where no `g`
  precedes it, it takes the name of the last `o`; failing that, group 0.
- **ASCII STL**: each `solid` block is a group, named by the block's name. Binary STL is
  all group 0.
- **3MF**: each object is a group, named by its `name` attribute, or by its `id` as
  written when it has no name. Every placement of an object shares its group.
  Per-triangle properties are not read.
- `Mesh::welded` and every placement keep each triangle's group. A breaking change,
  recorded in CHANGELOG.md.

**Lattice and ownership**

- The lattice is the corners of an SDF conversion's voxels: point v lies at origin + h·v,
  the SDF's sampling lattice shifted by −h/2. It is not set apart from the SDF.
- The data lives on that conversion's topology, under the same grid metadata. Point v
  owns its +x, +y and +z edges, which are edges of voxel v.
- A crossing on an edge of voxel v lies within √3/2·h of the voxel's centre. The voxel's
  stored distance is therefore below N·h for any band N ≥ 1, so every crossed edge has
  its owner in a leaf. This claim is held by a brute-force test, not by this record.

**Signs and crossings**

- **Signs**: each leaf point's sign is the winding number's, at the conversion's accuracy.
  Inside tiles count as inside, and the background as outside.
- **Which edges have a crossing**: an edge has one exactly when the stored signs of its
  two ends differ. This uses the stored signs, including those of tiles and background.
- **Finding it**: the edge is tested against every triangle with area, by Woop et al.'s
  watertight test. The hit nearest the edge's inside end is kept; on a tie, the lowest
  triangle index in file order wins.
- **Fraction**: the crossing's position from the edge's lower end.
- **Normal**: the triangle's unit normal, flipped where needed so that it points from
  the inside end toward the outside end. Where the normal is perpendicular to the edge,
  it is kept as the triangle's own.
- **Group**: the crossed triangle's group.
- **When no triangle is hit**: possible near a hole, an overlap, or an approximate
  winding number. The data then comes from the triangle with area nearest the edge, the
  crossing placed at the point of the edge nearest that triangle. Such edges are counted
  in the output; no flag is put in the data.
- **Degenerate cases**:
  - A lattice point whose computed distance to its nearest triangle is zero counts as
    inside, and its crossing then lies at a fraction of exactly 0 or 1.
  - Zero-area triangles are never crossed.
- **Determinism**: each point and edge is computed on its own, signs first and edges
  after. The bytes therefore do not depend on the thread count.

**Storage**

- **Fields**: per axis, an `f32` fraction, an `[f32; 3]` normal and a `u32` group; plus
  one `u8` sign field (1 inside, 0 outside). Values are computed in `f64` and stored in
  `f32`.
- **Size**: 3 × (4 + 12 + 4) + 1 = 61 bytes per leaf voxel, against 4 for the SDF.
- **Edges with no crossing, and tiles**: fraction −1, normal zero, group 0.
- **Cache file**: the ten fields go as ten grids, each with its own copy of the topology.
- **What it costs**: a per-triangle table with an `f64` fraction would have been exact
  and smaller (37 bytes), but needs a section the cache file lacks. The user accepted
  `f32`, being below their discretisation error.
- **One crossing per edge**: an edge crossed twice has ends of one sign and shows
  nothing. The user measured such edges at 0.3 to 1.2 % of crossed edges on a turned box,
  fewer as the grid is refined, and did not ask for them.

**Placement**

`colino::hermite` holds it. `colino::intersect` holds the edge test and the distance
between an edge and a triangle.

## Consequences

- colino's output gains a second kind of data beside the SDF, and ARCHITECTURE.md §5.1
  says so once this record is accepted.
- The three readers change, so each gets a fuzz round.
- Acceptance tests:
  - a cube turned about two axes, each face its own group: every crossing exact to
    `f32` rounding, every normal its face's, every group right;
  - a closed mesh converted at two thread counts gives the same bytes;
  - signs and crossings agree on every band edge of a torus;
  - by brute force, every crossed edge has its owner in a leaf;
  - an open mesh counts its edges with no triangle hit.
- Two things wait for a user's request: a table in the cache file, and every crossing
  along an edge.

## Amendment

- **Date**: 2026-10-22

The record left the API's shape and the cache file's grid names open. The human decided:

- `colino::hermite::hermite(mesh, &Converted, accuracy)` returns a `Hermite` or a
  `HermiteError`. `Hermite` holds `signs`, a `Field<u8>`; `axes`, three `Crossings`, for
  x, y and z; and `unhit`, the count of crossed edges with no triangle hit. `Crossings`
  holds `fraction`, a `Field<f32>`; `normal`, a `Field<[f32; 3]>`; and `group`, a
  `Field<u32>`. Every field lies on the conversion's topology, under its `GridMeta`.
- `Hermite::entries(&meta)` gives the ten `CacheEntry`s, named `sign`, then `x.fraction`,
  `x.normal`, `x.group`, and the same for `y` and `z`, in that order; the rest of each
  entry's metadata is `meta`'s. The names are part of colino's API, held by a Format-tier
  test against a committed file.
- `group_names` is indexed by group id. Empty, it stands for `[""]`; otherwise a first
  name other than `""` is a `MeshError`, as are the two cases above.
