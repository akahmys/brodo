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

## Amendment

- **Date**: 2026-10-17 (B-014)

Miri's Stacked Borrows reported undefined behaviour inside `crossbeam-epoch`, under
rayon, so B-014 asked whether `std::thread::scope` could replace rayon; `spikes/threads/`
measured it. A dynamic split of blocks of 16 leaves matched rayon on 10 469 leaves and
gave the same bits, but cost 40 to 50 µs a call to spawn, and under a caller's own rayon
loop of ten updates it was 8 % slower on 2606 leaves and about three times on 120, against
12 to 30 % with one inner thread each; nested rayon was fastest. Rayon with four threads
ran under Miri's Tree Borrows with no report. The human decided to keep rayon: the
leaf-parallel work runs in the caller's rayon pool, which ARCHITECTURE.md §4.4 states as
part of the design, so that a later change of backend is a breaking change and recorded
as one; and the parallel code is checked under Tree Borrows. The change stays cheap:
rayon is used in two functions of `parallel.rs`, and no rayon type is in brodo's API.


## Amendment

- **Date**: 2026-10-20

A user's stencil read up to three voxels away (ROADMAP.md §4, R9), and the spike
`spikes/ghost/` answered B-002 (its `REPORT.md`). On a sphere's band, a block of leaves and
a slab one leaf thick, each leaf voxel reading itself and ±1 to ±3 along each axis, a
gather three layers wide took 25.6 to 44.7 % less time than one layer with an accessor
beyond it, and 3.3 to 16.4 % less than two; reading beyond one layer through
`Field::value_at` cost up to 2.8 times an accessor. Looking up a leaf's neighbours took
1.2 to 2.9 % of a gather, and sharing it among several fields gained nothing beyond the
pairs' spread (R10). The human decided:

- `gather_wide` copies a leaf and `GhostWidth::{One, Two, Three}` layers around it into a
  caller's `Vec`, (8 + 2w)³ values x fastest; `GhostWidth` is `#[non_exhaustive]`. `gather`
  and its 10³ array stay.
- A read beyond the window goes through `ValueAccessor`, not `Field::value_at`.
- Several fields of a leaf are gathered one call each; a shared lookup is not added (R10).
