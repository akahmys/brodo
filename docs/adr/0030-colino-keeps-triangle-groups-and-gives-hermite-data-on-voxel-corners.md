# ADR-0030: colino keeps triangle groups and gives Hermite data on voxel corners

- **Status**: Amended
- **Date**: 2026-10-20

Condensed on 2026-11-08 (ADR-0001): each decision is kept with its reason and what it set
aside; the text before is in git, this file as of `53f69d5`.

## Context

A user building cut cells on brodo's grid asked for two things (ROADMAP.md §4, R11): a
group per triangle, read from the file, and Hermite data on a lattice: for each lattice
edge the surface crosses, where, the surface's normal there, and the crossed triangle's
group. A sampled SDF cannot hold the kink where two groups meet. The user answered
brodo's questions on 2026-10-20. `Mesh` then held vertices, triangles and a unit; the OBJ
reader skipped `g` and `o`; the 3MF reader kept no object identity once items were placed.

Read for this record:

- Ju, Losasso, Schaefer and Warren (2002), *Dual Contouring of Hermite Data*, ACM TOG
  21(3) 339–346, for Hermite data as signs at lattice points plus, on each edge whose
  ends differ, an exact crossing and normal.
- Woop, Benthin and Wald (2013), *Watertight Ray/Triangle Intersection*, JCGT 2(1) 65–82,
  for an edge test that misses no shared edge or vertex of adjoining triangles.

## Decision

The human decided, on 2026-10-20 unless dated otherwise.

**Groups**

- `Mesh` gains `groups`, empty (every triangle in group 0) or one `u32` per triangle, and
  `group_names`, indexed by id, empty standing for `[""]`. Group 0 is `""`, no group; a
  group is its name, ids given in the order names first appear. Another length, an id
  with no name, or a first name other than `""` is a `MeshError` (2026-10-22).
- **OBJ**: a triangle takes the names of the last `g`, joined by one space; with no `g`,
  or after a `g` naming none, the last `o`'s name; failing that, group 0 (2026-10-27: the
  specification says only that group information is optional).
- **ASCII STL**: each `solid` block is a group, named by the rest of its line after
  `solid`; on that line the name ends before a `facet` followed by `normal` or `outer`, or
  before an `endsolid` ending the line, so that a solid on one line reads; a name after
  `endsolid` ends with its line or before a `solid` on it. A line ends at `\n`, `\r\n` or
  a lone `\r` (2026-10-27: the Library of Congress's description, fdd000506, says a name
  may follow `solid`, not where it ends; the first rule refused names holding `facet` and
  lost solids written on one line). Binary STL is all group 0.
- **3MF**: each object is a group, named by its `name`, or its `id` as written. Every
  placement of an object shares its group. Per-triangle properties are not read.
- `Mesh::welded` and every placement keep each triangle's group.

**Hermite data, as colino produces it**

The lattice, the edges' owners, what signs, fractions, normals and groups mean, an edge
with no crossing, the types and the cache entries are brodo's format since 2026-10-22
(ADR-0032), which replaced this record's first API; what follows is colino's as a
producer. `colino::hermite` holds it, `colino::intersect` the edge test and an edge's
distance to a triangle; it returns brodo's set with its `unhit` count beside it.

- **Signs** are the winding number's, at the conversion's accuracy; inside tiles are
  inside, the background outside. A point whose distance to its nearest triangle is zero
  is inside.
- **A sign changing beyond the band** (2026-10-22). On a badly oriented mesh the winding
  number's inside can change where no leaf owns the edge: a cube with one face turned in,
  at a band of 3, put a false surface 4.07 voxels from the mesh. `hermite` refuses such a
  mesh, `HermiteError::SignOutsideBand`, naming the point; within the band the false
  surface's edges count as unhit. Adding leaves there, which would change conversion
  (ADR-0027), and answering B-016 first were set aside.
- **Finding a crossing**: an edge whose stored signs differ is tested against every
  triangle with area by Woop et al.'s watertight test; the hit nearest the edge's inside
  end is kept, the lowest triangle index on a tie. An inside end on the mesh takes the
  triangle holding it, the lowest-numbered with area at distance 0 (2026-11-02: it had
  taken a farther triangle's normal). The normal is the triangle's unit normal turned to
  point from the inside end outward, its own where perpendicular to the edge, divided by
  its largest component before its length (2026-11-02: the squared length underflowed
  below edges of about 10⁻⁸¹ and overflowed above 10⁷⁷).
- **The test in `f64`** (2026-10-22). Woop et al. compute in `f32`, recomputing an edge
  value of exactly 0 in `f64`; no wider type holds an `f64` product exactly, so an edge
  value whose two products round equal is decided by their rounding errors, each found
  exactly by `f64::mul_add`, and where the products are too small for that, the value is 0
  and counts as hit, as the paper counts a 0. Computing in `f32`, which could not give
  crossings exact to `f32` rounding, and counting every 0 a hit with no recomputation,
  further from the paper, were set aside. The third coordinate is compared unscaled with
  det·D, one division at the end (2026-10-25: scaling by 1/D put a hit at a segment's end
  past it). A triangle whose corners lie on a line, its own cross product zero, is refused
  before the test, as the parity count's is (2026-11-02: moved to the segment's start its
  corners rounded off the line, and 32 835 of 222 630 were crossed).
- **The search's margins are derived** (2026-11-02), not chosen by a test failing with one
  less: a corner moved by an edge's start rounds by at most 8ε of twice the largest
  coordinate, and an edge's box is widened by twice that (rounding had put 737 of 200 000
  triangles drawn one ulp past an edge's end inside it); a distance found falls short by at
  most √ε of the largest coordinate, and the nearest query's margin is twice that
  (ADR-0027).
- **No triangle hit**, near a hole, an overlap or an approximate winding number: the data
  comes from the triangle with area nearest the edge, at the edge's point nearest it, and
  the edge is counted, not flagged. Of triangles equally near within their distances'
  rounding, any one may be taken (2026-10-28; ARCHITECTURE.md §5.1).
- **Values** are computed in `f64` and stored in `f32`. A per-triangle table with an `f64`
  fraction, exact and smaller, needed a section the cache file lacks; the user accepted
  `f32`, below their discretisation error. One crossing per edge: an edge crossed twice
  shows nothing, 0.3 to 1.2 % of crossed edges on a turned box by the user's measure, and
  was not asked for.
- **Determinism**: each point and edge is computed on its own, signs first, so that the
  bytes do not depend on the thread count.
- **The set's metadata** (2026-10-27) is the conversion's voxel size, transform and unit,
  named `hermite`, without the SDF convention, so that its grids are not taken for SDFs;
  `Hermite::entries` refuses metadata claiming it (`ClaimsSdf`). Its reserved keys are
  ADR-0053's.

## Consequences

- colino's output has a second kind of data beside the SDF (ARCHITECTURE.md §5.1).
- Acceptance tests: a cube turned about two axes, each face a group, every crossing exact
  to `f32` rounding, every normal its face's and every group right; the same bytes at two
  thread counts; signs and crossings agreeing on every band edge of a torus; by brute
  force, every crossed edge owned by a leaf; an open mesh counting its unhit edges.
- A table in the cache file, and every crossing along an edge, wait for a user's request.
