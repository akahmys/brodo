# ADR-0027: colino converts by exact distance and fast winding numbers, into a band

- **Status**: Accepted
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
