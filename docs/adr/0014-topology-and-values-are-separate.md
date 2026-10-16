# ADR-0014: Topology and values are separate, and a topology edit returns a remap

- **Status**: Accepted
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
