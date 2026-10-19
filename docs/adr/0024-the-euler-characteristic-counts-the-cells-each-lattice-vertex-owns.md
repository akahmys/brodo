# ADR-0024: The Euler characteristic counts the cells each lattice vertex owns

- **Status**: Amended
- **Date**: 2026-10-18

## Context

colino's report (ARCHITECTURE.md §5.2, ADR-0017) needs the Euler characteristic χ of the
solid, from which β₁ = β₀ + β₂ − χ. §4.5 said only that χ is summed from a table of 2×2×2
local patterns, per leaf in parallel; which connectivity it follows, what it does with
tiles and the background, and what χ of an unbounded set is were open. Published
sources read: Sánchez-Cruz et al. (2013, J. Appl. Res. Technol. 11(1), 65–78) give
χ = n₀ − n₁ + n₂ − n₃ over the vertices, edges, faces and voxels of a voxelised object,
equal to its objects plus cavities minus tunnels; ImageJ Ops' documentation of
`EulerCharacteristic26N` sums a value per 2×2×2 neighbourhood, after Toriwaki and Yonekura
(2002, Forma 17, 183–209), whose full text was not reached.

## Decision

The human decided, on 2026-10-18:

- `euler(t, f, meets, conn)`, beside `label`. With `TwentySix`, χ of the union of the
  closed unit cubes of the voxels that meet the condition: a cell counts when any voxel
  around it meets it. With `Six`, of the open cubes joined across shared faces: a cell
  counts when every voxel around it meets it. Each lattice vertex owns itself and the 3
  edges, 3 faces and 1 cube on its positive side, all lying among the 8 voxels around it,
  so χ is a sum over vertices of a value of their 2×2×2 pattern. The two 256-entry tables
  are counted by the code from those cells, not copied. Which connectivity each table
  matches is shown by tests: two voxels meeting at a corner are one component under 26
  and two under 6, and χ agrees.
- A set that holds the background is unbounded: `AnalysisError::Unbounded`. colino needs
  χ of the solid only, which is bounded.
- A vertex among 8 voxels of one value contributes nothing. Vertices touching a leaf are
  summed per leaf, in parallel, from its gather with one ghost layer, each leaf owning the
  vertices at its own voxels' lower corners. Vertices among tiles, empty regions and the
  background only are found by descending the levels with an explicit stack, as labelling
  does, never voxel by voxel over a large region.
- The sum is an `i64`, so the result does not depend on the thread count.

## Consequences

χ comes in two units: over leaves, then across regions with no leaf. Its Reference test
counts n₀ − n₁ + n₂ − n₃ on a dense grid directly. B-011 still holds back dilation and
erosion.

## Amendment

- **Date**: 2026-10-19

Before χ was built, a brute-force count found three points of the decision wrong or
costlier than needed, and the human decided them:

- Under 6-connectivity the cells counted form the dual complex: a lattice vertex, an
  edge, a face and a cube stand for a cube, a face, an edge and a voxel, so each counts
  with the sign of dimension 3 − d. With the sign of d, a box came out −1.
- "Each leaf owning the vertices at its own voxels' lower corners" missed a vertex whose
  own voxel is in a tile and another of its 8 voxels in a leaf. A vertex whose 8 voxels
  hold a leaf voxel is counted by the leaf holding the first of them, x fastest, from
  that leaf's gather.
- Away from leaves, the voxels meeting the condition are disjoint aligned boxes, the
  tiles that meet it. A vertex inside one or within a face of one sees a flat pattern and
  contributes nothing, so only the vertices on the edges of those tiles are summed,
  12 × the edge per tile, instead of descending the levels. On 300 random unions of boxes
  the edge sum equalled the sum over every vertex, under both tables, χ ≠ 0 in 599 of the
  600 counts. The shapes counted: one voxel 1; two meeting at a corner or an edge 1 under
  26 and 2 under 6; at a face 1; a 5³ shell 2; a ring 0.
