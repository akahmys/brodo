# ADR-0051: Hermite data holds its crossings sparsely

- **Status**: Accepted
- **Date**: 2026-11-03

## Context

Hermite data at format version 1 is ten dense fields on one topology (ADR-0032,
docs/formats.md §4): per leaf voxel a sign and, per axis, a fraction, a normal and a group,
about 61 bytes a voxel, though only the edges a surface crosses hold anything. P5's review
found that a sphere of radius 1500 voxels would not fit in 32 GB (ROADMAP.md §3, B-023).

The spike `spikes/hermitesparse/` (2026-11-03, `spikes/hermitesparse/REPORT.md`) made an
analytic sphere's Hermite data on a narrow band's topology, dense as brodo makes it and
sparse: the sign field kept, and per leaf and axis a mask of the edges holding a crossing,
where each leaf's crossings start, and the crossings packed, 20 bytes each. Sparse took 15
to 17 times less memory and about 19 times fewer bytes, and filled and wrote 7 to 10 times
faster; at radius 1500 it peaked at 3.6 GB, where dense would have taken about 51.6 GB.
The sign field was a third of the sparse file.

A cache file holds grids only, each a frozen buffer of one value per voxel (docs/formats.md
§3), so crossings held sparsely have no place in it at version 1. B-022 raises the frozen
layout's and the cache profile's versions for checks of its own (ROADMAP.md §2, P8).

## Decision

The owner chose, on 2026-11-03, the first option of each point, as proposed. Asked
whether a user with memory to spare is better served dense, the owner kept sparse: dense
is slower to fill, write and check at any memory, its files about 19 times larger; its
advantages, each field a grid other tools show and an edge rewritten in place, are an
API's to give, a field expanded from the set, not the format's.

- **a. Whether**: the crossings are held sparsely, from Hermite data's version 2. Kept
  dense, the format cannot hold the shapes B-023 names on a machine of 32 GB.
- **b. The layout**: (1) per leaf and axis a 512-bit mask of the edges holding a crossing,
  where each leaf's crossings start, and the crossings in leaf, axis and place order,
  fraction `f32`, normal three `f32`, group `u32`; an edge's crossing found by its mask's
  rank, in constant time. (2) The crossings sorted by leaf, axis and place, each with its
  key, 24 bytes, found by a binary search: no masks, the same size on a sphere, slower to
  find. (3) A `u32` index per voxel and axis into a table of crossings: 12 bytes a voxel,
  more than the whole sparse file.
- **c. The signs**: (1) brodo's `u8` field as now, read as a `GridView`, its tiles' signs
  as they are, every check of §4.2 kept. (2) Bits, 64 bytes a leaf, 27 % less at radius
  1500, at the cost of a value type or a section of their own.
- **d. Where they live in a file**: (1) The cache profile's version 2, which B-022 raises
  anyway, gains an entry of named bytes beside its grids, with free pairs as a grid's; a
  set is its sign grid and one such entry holding the masks, starts and crossings, told
  apart by keys as now. One file keeps several sets and other grids, as ADR-0032 chose.
  The archive profile's version 2 carries such entries too. (2) A profile of its own for
  Hermite data, with its own header and media type: one set a file, and a reader of its
  own. (3) The crossings packed into a grid of bytes: a frozen buffer misused, its
  checks meaningless.
- **e. Versions**: Hermite data's version 2. A release reads version 1 as well and writes
  it on request (TESTING.md V6), so the dense writer is kept beside the sparse one.
- **f. The API**: `Hermite` holds the crossings sparsely; `Crossings`' three fields are
  replaced by a type that gives an edge's crossing, and a builder that takes them leaf by
  leaf. colino's producer fills the builder, never a dense field. This changes public
  items of brodo and colino, in CHANGELOG.md.

## Consequences

Hermite data, the cache profile and the archive profile each reach version 2 together
with B-022's checks, each release reading the version before it and writing it on request
(TESTING.md V6). docs/formats.md states the entry of named bytes and Hermite data's
version 2 before the code; the golden files of version 2 join §7's, those of version 1
kept. A field of one axis expanded from a set, for a tool that shows grids, is offered by
the API. A user who wants the dense format kept beyond V6's reach is a request for
ROADMAP.md §4.
