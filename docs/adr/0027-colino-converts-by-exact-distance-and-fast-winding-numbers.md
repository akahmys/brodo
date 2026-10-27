# ADR-0027: colino converts by exact distance and fast winding numbers, into a band

- **Status**: Amended
- **Date**: 2026-10-20

## Context

P2's third item turns a mesh into a narrow-band signed distance field (ARCHITECTURE.md
§5.1, ADR-0017). Read: Jacobson, Kavan and Sorkine-Hornung (2013), *Robust Inside-Outside
Segmentation using Generalized Winding Numbers*, ACM TOG 32(4) 33, for the winding number
as a sum of triangles' solid angles over 4π, each by van Oosterom and Strackee's formula
(their equation 6), and its threshold of 1/2; Barill, Dickson, Schmidt, Levin and Jacobson
(2018), *Fast Winding Numbers for Soups and Clouds*, ACM TOG 37(4) 43, for its
evaluation on a tree of axis-aligned boxes with a far-field Taylor expansion to second
order (Algorithm 1, equations 18 to 29, appendices A and B), which with β = 2 gave a
root-mean-squared error near 10⁻³ and the exact voxelisation at the threshold of 1/2.

## Decision

The human decided, on 2026-10-20:

- The caller gives the voxel size h; the grid's origin is the world's, unless the caller
  gives another, so that conversions of different inputs share a lattice. A voxel's value
  is taken at its centre, its index plus one half, in the mesh's world coordinates.
- Values are signed distances in the mesh's unit, as `f32`, negative inside, clamped to
  ±N·h, N being the band's half-width in voxels, 3 by default. Inside tiles hold −N·h; the
  background is +N·h.
- A leaf-sized cell becomes a leaf when one of its voxels' centres lies nearer the surface
  than N·h. Candidate cells are those overlapping a triangle's box grown by N·h.
- Distance is exact, to the nearest triangle, through a tree of axis-aligned boxes split
  at the median, walked with an explicit stack. Sign is inside where the winding number is
  1/2 or more, by Barill et al.'s approximation with β = 2 to second order, or by the
  exact sum when the caller asks.
- Regions holding no leaf are classified from the largest down, root region, upper node,
  lower node, by one winding number at the region's centre; an inside region becomes one
  tile at its level.
- The grid's metadata takes the mesh's unit, or the caller's when the mesh has none, and
  is an error when neither gives one.
- `colino::bvh`, `colino::distance`, `colino::winding` and `colino::convert` hold it; leaves
  are filled in parallel through brodo's leaf-parallel update, each voxel on its own, so
  that the bytes do not depend on the thread count.
- The orientation-independent option ADR-0017 names is left out of this item: what it is
  is open, B-016 (ROADMAP.md §3).

## Consequences

Memory follows the band, plus one tile per inside region. Conversion comes in three
units: distance and the tree, the winding number, then the band.

## Amendment

- **Date**: 2026-10-26

B-016 asked which sign colino offers beside the winding number's that does not depend on
orientation. `spikes/sign/` signed dense grids of broken meshes by six methods against
the intended mesh's exact winding number (its REPORT.md): no method held on every defect.
The winding number fails on turned faces; a flood from outside leaks through a hole of
one triangle and fills cavities; ray stabbing (Nooruddin and Turk, 2003, *Simplification
and Repair of Polygonal Models Using Volumetric Techniques*, IEEE TVCG 9(2), §3.2) fills
cavities; parity counting along lines in three axis directions with a majority vote, a
line with an odd count not voting and a tie outside (§3.1), held on turned faces, holes
and cavities, and mixes of them, but fails where parts pass into each other or walls are
doubled, where the winding number holds. colino's `crossing` hits both triangles of an
edge a line passes exactly on, so parity needs a crossing counted once there. The human
decided:

- The orientation-independent option is the parity count in the three axis directions;
  the winding number stays the default. Ray stabbing and the others are not offered.
- Conversion and Hermite data take the same choice of sign, Hermite data's signs on the
  voxels' corners counted as conversion's are on their centres (ADR-0030).
- How a line exactly on a shared edge or corner counts once is measured before the option
  is built, between moving the lines off the lattice by a fixed offset and a half-open
  rule on the edge functions, and recorded here.

The spike then measured both (REPORT.md, "A line on an edge"). An edge function exactly 0
taking the sign it has with the line moved by (ε, ε²) across itself, each product's sign
exact, counted every line on an edge or a corner once, on a cube's split faces and on an
octahedron with its vertices on the lines, and changed no other sign. A fixed offset of
the lines signed the moved point rather than the voxel's centre and missed 20 voxels
nearest each sphere. The human chose the perturbation (2026-10-26): parity counts with an
edge test of its own beside `crossing`, which keeps hitting both triangles of an edge for
Hermite data's crossings.

Building it found one limit the spike's holes were too shallow to show: within the sag of
a missing cap, a point whose lines across pass through the hole finds no crossing on them
and is outside. A sphere with a hole and a tenth of its faces turned keeps every sign below
the hole's rim (`sign_reference`).

## Amendment: a point on the mesh

- **Date**: 2026-10-27

P5's review found two faults at a voxel centre exactly on the mesh. Its winding number
turned on the sign of a rounded zero, the solid angle of the triangle holding it being
2·atan2(±0, d) with d < 0, so the winding number and parity converted a cube with its
faces on planes of centres differently in 206 values (2026-10-26); and an inside one held
−0, which `v < 0` does not find, so two boxes sharing a face labelled as two parts. The
human decided that a point at distance exactly 0 takes the parity count's sign, decided by
its lines moved by (ε, ε²), whichever sign is asked for, and that an inside voxel whose
distance rounds to zero holds −`f32::MIN_POSITIVE`. Hermite data keeps its own rule: a
point on the mesh is inside, so that the crossing lies there.
