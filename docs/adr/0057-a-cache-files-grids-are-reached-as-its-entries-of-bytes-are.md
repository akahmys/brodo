# ADR-0057: A cache file's grids are reached as its entries of bytes are

- **Status**: Amended
- **Date**: 2026-11-05

## Context

A survey of designs grown by accretion before 1.0 (ROADMAP.md P9) found a read cache file's
grids reached by number, `grid_count`, `meta(i)`, `bounds(i)`, `value_type(i)` and
`view(i)`, with `thaw(i, limit)` and `thaw_size(i)` added by ADR-0056, while its entries of
bytes, added with version 2 of the profile, came as a list, `bytes_entries`. It found too
that `CacheEntry::with_reserved_keys` and `CacheEntry::bytes_with_reserved_keys`, which
write keys beginning `brodo.`, serve brodo's own Hermite data and tests that make broken
files, yet were public items 1.0 would promise.

## Decision

The owner decided, on 2026-11-05, as proposed:

- `CacheFile::grids` gives the grids in the file's order, each a `GridEntry`, a handle
  that copies, with `index`, `meta`, `bounds`, `value_type`, `view`, `thaw_size` and
  `thaw`; the accessors taking a grid's number are gone, so that a grid is reached one way.
  `CacheError::Grid` and `ThawTooLarge` name the grid by its `index`. `archive::read_region`
  keeps its grid number, reading no file whole.
- `with_reserved_keys` and `bytes_with_reserved_keys` stay public, for brodo's own formats
  and for tests outside the crate, but `#[doc(hidden)]`, promising nothing. `SDF_KEY`
  stays the one reserved key `CacheEntry::new` writes (ADR-0053).

## Consequences

Every caller of the accessors by number changes; CHANGELOG.md lists them, and its recipe
for converting a Hermite set of version 2 names the hidden writers it uses. A grid number
out of range is no longer an error a `CacheFile` gives: `grids().get(i)` is `None`. The
bytes are unchanged.

## Amendment

- **Date**: 2026-11-09

Issue 14 reported that a whole read named a refused grid by its place in the directory,
entries of bytes counted, where `GridEntry::index`, a region's read, `view` and `thaw`
named it by its place among the grids, and that an archive's chunk errors mixed the two;
CHANGELOG.md's 1.0.0 section still stated the directory's place, written before this
record. A test of a file whose entry of bytes came before its grid measured it. The owner
decided, on 2026-11-09, as proposed:

- `CacheError::Grid`, `Decompress` and `Chunks` name a grid by its place among the file's
  grids, as `GridEntry::index` numbers it, from every read, whole or of a region. It was a
  fix to what this record promised, not a change to it.
- An archive's entry of bytes whose chunk does not decompress, or that is not cut as the
  format cuts it, is named as one, by its place among the file's entries of bytes as
  `CacheFile::bytes_entries` lists them, by two variants of their own,
  `CacheError::BytesDecompress` and `CacheError::BytesChunks`: `CacheError` is
  non-exhaustive, so that adding them breaks no user, where a field added to a variant
  would.
