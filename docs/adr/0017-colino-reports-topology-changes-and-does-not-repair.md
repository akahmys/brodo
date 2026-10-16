# ADR-0017: colino converts meshes to narrow-band SDFs and reports topology changes without repairing them

- **Status**: Accepted
- **Date**: 2026-10-16

## Context

Outside meshes are often badly oriented or holed. Voxelising can close a thin gap or open
a thin wall, and a global count can hide one change behind another. Whether to repair or
refine is the user's decision, not the converter's. A dense grid at 3000³ does not fit in
memory.

## Decision

- Distance is exact, to the nearest triangle through a BVH. Sign is by generalised
  winding number with a hierarchical approximation (Jacobson et al., 2013; Barill et al.,
  2018), with an orientation-independent option.
- Only a band of ±N voxels (3 by default) becomes leaves; inside is negative tiles,
  outside the positive background. No dense grid is built: memory follows the band.
- The report has three layers: Betti numbers β₀, β₁, β₂ of both the solid and its
  complement (β₁ from the Euler characteristic); whether user-labelled regions share a
  complement component; and places thinner than two voxels, predicted from the input.
- The reference is the input's own Euler characteristic for a closed manifold, and
  otherwise a local reconversion of the risky places at h/2.
- The report is a `serde`-serialisable structure stating what may have changed, where
  and by how much. colino holds no CFD concept.

## Consequences

Inputs come in the order STL, OBJ, 3MF; STEP is later. The third layer is after P2.
