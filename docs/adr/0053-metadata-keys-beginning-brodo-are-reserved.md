# ADR-0053: Metadata keys beginning `brodo.` are reserved

- **Status**: Accepted
- **Date**: 2026-11-05

## Context

A cache file's grids and entries of bytes carry free pairs (docs/formats.md §3.3). Before
this decision brodo gave a meaning to keys of two prefixes: Hermite data's, beginning
`hermite.` (ADR-0032), which `CacheEntry::new` and `CacheEntry::bytes` refused so that a
grid was not taken for a set's, and the SDF convention's `sdf.version` (ADR-0039). A
later format built on the cache profile after 1.0 would need keys of its own, and
refusing them then would break a caller that writes them. The review after 0.9.0 raised
this (ROADMAP.md P9), and the owner decided on 2026-11-04 that one prefix be kept before
1.0.

Two ways were weighed. (A) Keep `brodo.` for later formats, `hermite.` and `sdf.version`
as they were: no file's bytes change, but three prefixes stay brodo's, `hermite.` a common
word another tool may use. (B) Move every key brodo gives a meaning under `brodo.`: one
prefix, `hermite.` and `sdf.` given back to callers, but Hermite data's and the SDF
convention's versions raised. Under TESTING.md V6 a release raising a version still reads
the one before, so (B) after 1.0 would carry the old keys' reading for good; before 1.0 it
need not. A file of the earlier versions converts mechanically: Hermite data of version 2
by renaming its keys, of version 1 by its dense fields made sparse, and an SDF by renaming
one key.

## Decision

The owner chose (B) on 2026-11-05, dropping the earlier versions rather than reading them,
and a recipe in CHANGELOG.md rather than a converter in `mestolo`:

- Keys beginning `brodo.` are brodo's formats'. `CacheEntry::new` refuses one but
  `brodo.sdf.version`, `CacheEntry::bytes` every one, and `Hermite::entries` every one,
  with `ReservedKey`, the SDF convention's with `ClaimsSdf`;
  `CacheEntry::with_reserved_keys` and `CacheEntry::bytes_with_reserved_keys` write one,
  for such a format and for files made to break one. Keys beginning `hermite.` and
  `sdf.` are free.
- Hermite data is at version 3: version 2's sign grid and entry of crossings, its keys
  `brodo.hermite.version`, `brodo.hermite.set`, `brodo.hermite.field` and
  `brodo.hermite.group.<id>` (docs/formats.md §4). Versions 1 and 2 are neither read nor
  written: `Hermite::entries_v1`, `FIELDS`, `VERSION_1`, `HermiteView::version` and
  `HermiteError::OtherMeta` are gone, and a set whose entries name it by `hermite.set` is
  refused with `Version`, naming the version its `hermite.version` states.
- The SDF convention is at version 2, version 1's with its key `brodo.sdf.version`
  (docs/formats.md §5); `sdf.version` is not read.
- TESTING.md V6 lets the owner set its rule aside before 1.0, in an ADR (ADR-0038's
  amendment); this is the one use.
- The frozen layout and the cache and archive profiles keep their bytes and versions, and
  still read version 1.

## Consequences

A file written by 0.9.0 or earlier holding a Hermite set, or an SDF read by its key, is
not read as one until converted by its recipe (CHANGELOG.md); its grids are read as
before. A format brodo builds on the cache profile after 1.0 names its keys
`brodo.<format>.…` without breaking a caller. A grid read from a `.vdb` keeps the file's
string metadata in its free pairs, so one holding a key beginning `brodo.` is refused by
`CacheEntry::new`; `vdb::from_bytes` says so, and how to keep or drop such a key. The
golden files `hermite_v1.bin` and `hermite_v2.bin` stay, as sets refused by their version
(docs/formats.md §7).
The mutant excluded as equivalent in `Owned::crossing_tile` (ADR-0023) went with the
function, a set in memory or read having no tile's crossing to look up.
