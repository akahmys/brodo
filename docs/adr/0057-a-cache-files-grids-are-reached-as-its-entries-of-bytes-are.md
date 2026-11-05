# ADR-0057: A cache file's grids are reached as its entries of bytes are

- **Status**: Accepted
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
