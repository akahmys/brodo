# ADR-0024: The Euler characteristic counts the cells each lattice vertex owns

- **Status**: Accepted
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
