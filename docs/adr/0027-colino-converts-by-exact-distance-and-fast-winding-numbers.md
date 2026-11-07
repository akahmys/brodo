# ADR-0027: colino converts by exact distance and fast winding numbers, into a band

- **Status**: Amended
- **Date**: 2026-10-20

Condensed on 2026-11-08 (ADR-0001): each decision is kept with its reason and what it set
aside; the text before is in git, this file as of `53f69d5`.

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

The human decided, on 2026-10-20 unless dated otherwise.

- **The grid.** The caller gives the voxel size h; the grid's origin is the world's unless
  the caller gives another, so that conversions of different inputs share a lattice. A
  voxel's value is taken at its centre, its index plus one half, in the mesh's world
  coordinates. Its metadata takes the mesh's unit, or the caller's when the mesh has none,
  and is an error when neither gives one.
- **Values** are signed distances in the mesh's unit, as `f32`, negative inside, clamped
  to ±N·h, N the band's half-width in voxels, 3 by default; inside tiles hold −N·h, the
  background +N·h. An inside voxel whose distance rounds to zero holds
  −`f32::MIN_POSITIVE` (2026-10-27: −0 is not found by `v < 0`, and two boxes sharing a
  face labelled as two parts).
- **Leaves.** A leaf-sized cell overlapping a triangle's box grown by N·h is a candidate
  when its centre lies within (N + 4√3)·h of the triangle, 4√3·h the distance from a cell's
  centre to its corners, and stays a leaf when one of its voxels' values lies within the
  band (2026-10-27, as the code had it; ARCHITECTURE.md §5.1). Regions holding no leaf are
  classified from the largest down, root region, upper node, lower node, by one winding
  number at the region's centre; an inside region becomes one tile at its level.
- **Distance** is exact, to the nearest triangle, through a tree of axis-aligned boxes
  split at the median, walked with an explicit stack. Above a threshold of thinness, the
  normal's length against ε of the edges' product measured at the first corner, the
  face's nearest point is `p` projected on the triangle's plane; below it, the edges'
  (2026-11-02: barycentric weights, ratios of |n|², lost ε over the square of the
  triangle's sine, up to 0.13 of its size on thin triangles against points worked in
  rationals). The edges and `p` are divided by their largest component first, so that no
  product of four lengths under- or overflows. Turning the triangle so that its largest
  angle came first was tried and taken back on 2026-11-03: its code and tests cost more
  than it bought, which lay beyond what `on_triangle` promises for a triangle thinner than
  √ε of its size, a point within its width (ARCHITECTURE.md §5.1).
- **Sign.** Inside where the winding number is 1/2 or more, by Barill et al.'s
  approximation with β = 2 to second order, or by the exact sum when the caller asks;
  `Converted::accuracy` records which, and Hermite data and the report follow it, as they
  follow `Converted::sign` (2026-10-27). A point at distance exactly 0 is inside, whichever
  sign is asked for, as Hermite data takes it (2026-10-27: the winding number turned on
  the sign of a rounded zero, and the parity count's sign there split overlapping parts
  and put a box's lower faces inside and its upper ones outside).
- **A sign without orientation** (2026-10-26, B-016). `spikes/sign/` signed broken meshes
  by six methods against the intended mesh's exact winding number: none held on every
  defect. The winding number fails on turned faces; a flood from outside leaks through a
  hole and fills cavities; ray stabbing (Nooruddin and Turk, 2003, *Simplification and
  Repair of Polygonal Models Using Volumetric Techniques*, IEEE TVCG 9(2), §3.2) fills
  cavities; parity counting along lines in three axis directions with a majority vote, a
  line with an odd count not voting and a tie outside (§3.1), held on turned faces, holes
  and cavities, failing where parts pass into each other or walls are doubled. The parity
  count is offered beside the winding number, which stays the default; the others are
  not. Conversion and Hermite data take the same choice. A line exactly on an edge or a
  corner counts once by an edge function of exactly 0 taking the sign it has with the line
  moved by (ε, ε²), each product's sign exact, in an edge test of the parity count's own;
  a fixed offset of the lines, the other candidate, signed the moved point and missed 20
  voxels nearest each sphere.
- **Placement.** `colino::bvh`, `colino::distance`, `colino::winding` and
  `colino::convert` hold it; leaves are filled in parallel through brodo's leaf-parallel
  update, each voxel on its own, so that the bytes do not depend on the thread count.

## Consequences

- Memory follows the band, plus one tile per inside region.
- An open mesh's inside is not defined beyond the band: across a hole a region of up to
  4096³ voxels takes its centre's sign. The report counts its open edges; refining regions
  whose corners disagree, which would add leaves and time to every conversion, was set
  aside (2026-10-27). Within the sag of a missing cap, a point whose axis lines pass through
  the hole is outside by parity.
- The winding number holds for a mesh between 10⁻⁴⁵ and 10⁶⁰ in size by the approximation,
  10⁻¹⁰⁰ and 10¹⁰⁰ by the exact sum (2026-10-27, a sphere at every fifth power of ten).
  The range is stated, not enforced nor widened by scaling, which would change the bits of
  every result.
- A distance is within the rounding of the coordinates over the sine of the triangle's
  sharpest angle, below √ε of its size near the threshold.
