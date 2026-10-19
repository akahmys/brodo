# ADR-0025: Dilation and erosion work on whole leaves, without a per-voxel mask

- **Status**: Accepted
- **Date**: 2026-10-19

## Context

ARCHITECTURE.md §4.5 promised dilation and erosion of topology and values; B-011 asked
whether they need a per-voxel active mask in each leaf, which holds none (ADR-0013). A
shape kernel needs dilation and pruning (§1). `spikes/dilate/` compared, on narrow bands
of half-width 3 around spheres of radius 80 to 500, a torus and a plate thinner than a
voxel: A, whole leaves active, against B, a 512-bit mask per leaf with exact morphology.
A was sound in all 80 cases against the exact morphology of its own voxels and left every
solid's Euler characteristic unchanged when dilating; after dilating by k ≤ 3 it held
1.2 to 1.7 times B's memory on curved bands and 2.4 times on the plate, and eroding by 1
it kept 29 to 64 % of the exact erosion of its own set.

## Decision

The human decided, on 2026-10-19:

- B-011: no mask. A leaf stays wholly active.
- Topology: `dilate` adds a leaf at every background cell holding a voxel within k steps,
  6- or 26-connected, of a voxel of a leaf or tile; `erode` removes every leaf holding a
  voxel within k steps of the background, and splits a larger tile only where it must,
  removing the leaves that result. Both return a topology with its `Remap`, so fields
  follow as after any edit; a new leaf starts from the tile's value or the background.
  The result contains the exact dilation and lies within the exact erosion.
- Values: a max (dilation) or min (erosion) over the 6- or 26-neighbourhood, k times, of
  a field on a fixed topology, through the gather and a double-buffered leaf-parallel
  update. Only leaf voxels are written; tiles and the background are read, not changed.
  A caller wanting exact voxel reach dilates the topology first, then the values.

## Consequences

Nothing that reads a leaf changes, nor the frozen layout or the cache format. Dilation is
coarse, the whole ring of neighbouring leaves for 26-connected k ≥ 1, and erosion
removes much more than the exact one; pruning afterwards, by merging uniform leaves into
tiles, is the caller's. brodo gains a way to remove a leaf, which erosion needs.
