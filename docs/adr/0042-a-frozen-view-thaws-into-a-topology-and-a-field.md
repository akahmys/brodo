# ADR-0042: A frozen view thaws into a topology and a field

- **Status**: Amended
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

## Amendment: the Hermite reader refuses shared nodes too

- **Date**: 2026-10-27

The decision above left `GridView::new` and the Hermite reader to version 1's checks,
refusing shared nodes waiting for a version raised for another reason. P5's review then
found that the Hermite reader, walking the whole tree, asked much work of a small file
whose nodes were shared, and ADR-0038's amendment of 2026-10-27 has it refuse them now,
by the walk thawing uses (docs/formats.md §4.2, rule 11). `GridView::new` and the cache
reader keep version 1's checks.

## Amendment: a view's leaves and tiles are listed without thawing

- **Date**: 2026-11-09

Issue 13, approved by the owner, asked for where a view's leaves and tiles lie without
thawing it: a reader converting a mapped grid to another layout copied every value to
learn each leaf's origin and each tile's level, origin and value. The Consequences above
had set option 2, iterating them in public, aside, thawing serving the use then.

The owner decided, on 2026-11-09, as proposed:

- `GridView::regions` is public and gives a `ViewRegions<T>`: each leaf's origin at its
  leaf number, and each tile's level, origin and value at its place among the tile values,
  the number `ViewLocation::Tile` gives. They are what `thaw` gives; the nodes are walked
  once and no leaf value is copied.
- `ViewRegions` keeps its fields private behind `leaves` and `tiles`, which give slices, so
  that what it holds may grow without breaking a user; a tile is a tuple, as
  `Topology::tile` gives one. Two iterators were set aside: they would walk the nodes twice,
  and the order asked is not the walk's, so that each would gather the walk first.
- The walk thawing and the Hermite reader use is this one, so that one mechanism serves
  all three (PLANNING.md §2).
- It lands in 1.2.0, a milestone of its own, P11, after P10's closing checks and release.
