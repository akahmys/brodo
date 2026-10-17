# ADR-0015: Parallel updates and stencil gathers work per leaf, with a fixed-order mode

- **Status**: Amended
- **Date**: 2026-10-16

## Context

Stencil updates read neighbours of an old field and write a new one. The leaf is the
natural unit of parallel work, and results that depend on the thread count cannot be
compared between runs or solvers.

## Decision

- A leaf-parallel API (`rayon`) reads the old field by `&` and writes the new by `&mut`,
  one 512-value chunk per leaf.
- A gather copies a leaf and one ghost layer from its 26 neighbours into a dense 10³ array,
  filling missing neighbours with tile or background values; the width is to grow to two.
- `ValueAccessor` caches the last node path for nearby random access beyond the neighbour
  table, borrowing the topology and a field, one per thread.
- Reductions have a fixed-order mode whose result does not depend on the thread count.

## Consequences

The ghost width is open (ROADMAP.md §3, B-002). These structures are needed in P1 even
though the API is not fixed until later.

## Amendment

- **Date**: 2026-10-17

Three points were decided by the human before the update and the gather were built.

- The update's function may fail. Every leaf is visited, and the error returned is that
  of the lowest-numbered leaf that failed, so it does not depend on the thread count. A
  function that could not fail would have had to read other fields through views that are
  still fallible for a leaf number from another topology.
- The fixed-order reduction was built with them, as the diffusion example's conservation
  check needs it: a partial result per leaf in parallel, folded in leaf order.
- The gather takes tile values from the start, since the leaf manifest already names each
  neighbouring tile. Its width stayed one layer; B-002 was not answered.

## Amendment

- **Date**: 2026-10-17 (second)

Before `ValueAccessor` was built, the human decided that it writes only voxels of
leaves: a write to a tile's voxel would change up to 4096³ voxels at once, and a tile's
value already has its own setter. A write anywhere else is a typed error.
