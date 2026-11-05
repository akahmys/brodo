# ADR-0060: An entry read is written again as it was, and names need not be unique

- **Status**: Accepted
- **Date**: 2026-11-06

## Context

TESTING.md T14's survey before 1.0 (ROADMAP.md P9) found, by running brodo:

- A grid or an entry of bytes read from a cache file or an archive could be written again
  only by thawing and freezing it with `CacheEntry::new` or `CacheEntry::bytes`, which
  refuse keys beginning `brodo.` but the SDF convention's (ADR-0053). A Hermite set could
  not pass from a cache file to an archive, nor back, but through the hidden
  `with_reserved_keys` forms, which promise nothing (ADR-0057).
- Nothing stated whether a cache file's entries may share a name. The writer takes two of
  one, and a Hermite set's sign grid and entry of crossings are named alike; the `.vdb`
  writer refuses two grids of one name, OpenVDB telling grids apart by name.

## Decision

The owner decided, on 2026-11-05, as proposed:

- `CacheEntry::copy(GridEntry)` and `CacheEntry::copy_bytes(BytesEntry)` make an entry of
  one a file read, its metadata, free pairs and keys beginning `brodo.` kept, a grid's
  frozen buffer copied, neither thawed nor frozen again. They refuse nothing: what they
  copy was checked whole when the file was read (docs/formats.md §3.4).
- A cache file's entries need not have unique names; the profile tells them apart by their
  place, and a format built on it by its free pairs (docs/formats.md §3). The `.vdb`
  writer keeps refusing two grids of one name.
- The 0.9.0 migration's recipe, which renames keys and so writes with the hidden forms,
  is unchanged.

## Consequences

A Hermite set or an SDF read from a cache file or an archive is written to another whole,
to the same bytes when its entries keep their order; `hermite_format.rs` holds a set and an
SDF to that through a cache file and an archive. `CacheFile` lists grids and entries of
bytes apart, so a file whose kinds alternate is copied in another order, its entries the
same. The bytes of every format are unchanged.
