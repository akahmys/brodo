# ADR-0014: Topology and values are separate, and a topology edit returns a remap

- **Status**: Amended
- **Date**: 2026-10-16

## Context

A solver holds many fields (density, velocity, temperature) over one active region.
Values stored in the leaves would duplicate the topology per field and scatter memory.
Changing a topology shared by several fields silently invalidates them all.

## Decision

- `Topology` holds the tree, the leaf list and where tiles are; `Field<T>` holds one
  contiguous `Vec<T>` of leaf values (leaf count × 512), its tile values and its
  background. `Grid<T>` binds one topology and one field for single-field use.
- Topology changes go through an editor whose `commit` returns a `Remap` from old to new
  leaf numbers; every field on that topology must be remapped. Value updates and topology
  edits happen in separate phases.
- `LeafManifest` is a cache derived from the tree at each commit: leaf coordinates in
  Morton order, a coordinate-to-leaf table, and each leaf's 26 neighbours. Leaf numbers,
  and so field order, follow Morton order. The tree is the only source of truth.

## Consequences

How a stale field is refused by type (a generation number checked by the field is the
proposal) is open (ROADMAP.md §3, B-001).

## Amendment

- **Date**: 2026-10-16

The spike `spikes/remap/` answered B-001 and B-008 by measurement (its `REPORT.md`).

- **B-001.** Three ways to refuse a stale field were compared: a stamp per topology checked
  by each field, a topology that owns its fields, and a brand (an invariant lifetime)
  shared by a topology and its fields. The brand refused a stale field at compile time, but
  confined each topology to a closure: a time loop with commits became recursion one level
  per commit, which CODING.md Rule 15 refuses. The owning topology needed two workarounds
  in the slice's scenarios and type erasure. None cost measurably more than another. The
  stamp was chosen: every topology carries a stamp unique in the process, each field
  records the stamp it was made or remapped for, and any use with another topology, a
  remap included, is the typed error `StaleField`. `commit` leaves the old topology usable,
  so a refused remap leaves the caller its old topology and fields.
- **B-008.** `Remap` records where each new leaf's values come from (an old leaf, the tile
  it was split from, or the background) and each new tile's value (an old tile, a merged
  leaf, or the background). A split leaf takes each field's own tile value. A merged tile's
  value comes from a merge rule each field is given at remap, `Fn(&[T]) -> Option<T>`;
  `None` refuses the merge. brodo provides the rule that accepts only values with the same
  bits; any other is the caller's. Rules run only for merged regions that survive the
  commit, in leaf order.

Merging many leaves or tiles into a tile above leaf level was not measured; the spike's
tiles were leaf-sized. It is settled with tiles in full (ROADMAP.md §2).

## Amendment

- **Date**: 2026-10-17

The human decided, before `Topology` was built, that a topology numbers its leaves and
tiles in the Morton order of their origins and rebuilds the numbering at each commit, so
that field values are in their final order from the first field on. `LeafManifest` was
left with the 26-neighbour table only; the leaf list and the coordinate-to-leaf lookup
moved to `Topology`, which locates a voxel through the tree. The merge rule was left out
of `remap` until merges exist (B-010).
