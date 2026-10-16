# ADR-0015: Parallel updates and stencil gathers work per leaf, with a fixed-order mode

- **Status**: Accepted
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
