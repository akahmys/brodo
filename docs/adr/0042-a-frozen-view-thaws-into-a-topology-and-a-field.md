# ADR-0042: A frozen view thaws into a topology and a field

- **Status**: Accepted
- **Date**: 2026-10-26

## Context

A user's solver reads geometry callers give as cache files, Hermite sets and SDFs among
them, checks them as untrusted, and computes on them with gathers, `par_reduce` and
`Hermite::new`, which take an owned `Topology` and `Field<T>`, not a `GridView` (ROADMAP.md
§4, R12). A view gives values voxel by voxel and a leaf's values by its number, but not
where its leaves and tiles lie: a reader rebuilds the tree by `locate` over the grid's
bounds, at a cost that follows their volume, and cannot tell whether two grids of a file
share a tree, `same_topology` being the crate's own.

The crate's walk of a view's nodes, `regions`, costs what the nodes and leaves hold. Version
1's reader does not check that each node and leaf is reached once (docs/formats.md §2.5,
B-022): root entries naming one upper node make the walk repeat it, a small file asking for
much work, and keep one of a shared leaf's origins only (ROADMAP.md §2, P5's review, F2
and F4).

## Decision

The human decided, on 2026-10-26, the request being taken:

- `GridView::thaw` gives the view's tree and values as an owned `(Topology, Field<T>)`;
  `GridView::thaw_on(t)` gives the view's values as a `Field<T>` on a topology `t` that
  holds the view's leaves and tiles, refusing another with
  `FormatError::OtherTopology`; `GridView::same_topology` is public.
- The topology numbers leaves and tiles in its own Morton order, not the file's, so that
  one file gives one topology and one numbering.
- Thawing walks the view's nodes once and refuses, with `FormatError::Shared`, a view in
  which a node, a leaf or a tile value is reached more than once: its cost then follows the
  file's size. brodo writes no such file. `GridView::new` and the Hermite reader keep
  version 1's checks; refusing shared nodes there waits for a version raised for another
  reason (ADR-0038, B-022).
- Thawing is a reader of untrusted bytes: a malformed view gives a typed error, never a
  panic, and the fuzz targets reach it.
- It lands in 0.6.0, P5's version, after the review's fixes to the Hermite reader.

## Consequences

The user computes on a file's grids by thawing one and the others on its topology, at a
cost that follows the file. Option 2 of the request, iterating a view's leaves and tiles
in public, is not taken: thawing serves the use with a smaller API.
