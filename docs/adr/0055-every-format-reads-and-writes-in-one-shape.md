# ADR-0055: Every format reads and writes in one shape

- **Status**: Amended
- **Date**: 2026-11-05

## Context

A survey of designs grown by accretion before 1.0 (ROADMAP.md P9) found the readers and
writers of brodo's formats each shaped as it was added. A cache file was read by
associated functions, `CacheFile::read(path, limit)` and `CacheFile::from_bytes(bytes)`,
an archive and a `.vdb` by free ones; `archive::region_from_bytes` took its limit second
and `vdb::from_bytes` last; a `.vtu` was written only to a `Write` (`vtk::write_vtu`) and
a PNG only made as bytes (`slice::encode_png`), so that mestolo carried its own copy of
the whole-file replace `cache::write` uses. colino's mesh readers took bytes under the name
`read`, which brodo's readers give to a path.

## Decision

The owner decided, on 2026-11-05, that each format's module read and write in one shape:

- `from_bytes(bytes, …)` reads bytes and `read(path, …)` a file, free functions of the
  format's module; `CacheFile::map` stays, mapping being the cache's alone.
- A limit is the last argument wherever reading makes more than its input:
  `archive::read_region(path, index, extent, limit)` and
  `archive::region_from_bytes(bytes, index, extent, limit)`.
- `to_bytes(…)` makes the bytes and `write(path, …)` writes a file whole, to a hidden file
  renamed over it; the archive keeps `to_bytes_with` and `write_with`, which take an
  encoding per grid. `vtk::write_vtu` becomes `vtk::to_bytes` and `vtk::write`, and
  `slice::encode_png` becomes `slice::to_bytes`, with `slice::write` beside it.
- colino's `stl`, `obj` and `threemf` read bytes by `from_bytes` and a file by `read`,
  which refuses a file longer than `Limits::bytes` before reading it and reads no more
  than the limit and one byte, as brodo's readers do (ADR-0036). `ReadError` gains `Io`,
  holding the `std::io::Error`, and so no longer derives `Clone`, `PartialEq` or `Eq`.
- mestolo writes through brodo's writers and keeps only its refusals: a file at the path
  without `--force`, and a path naming no file.

## Consequences

Every caller of a renamed function or reordered argument changes; CHANGELOG.md lists them.
`SliceError` gains `Io` for `slice::write`. A `.vtu` is no longer written to a `Write` the
caller holds: `to_bytes` gives the bytes to write anywhere. A test comparing a `ReadError`
compares its `Debug` text; colino's `each_damage_is_its_own_error` for STL, longer for it,
is split into `each_damage_to_binary_is_its_own_error` and
`each_damage_to_ascii_is_its_own_error`. The mutant excluded in `CacheFile::read`'s
arithmetic (ADR-0023) is the same one in `cache::read`, its exclusion renamed with it.

## Amendment

- **Date**: 2026-11-10

Issue 2 asked for an archive listed without decompressing it and for grids found by name.
The owner decided, on 2026-11-10, as proposed, that the new reads take this record's
shape: `archive::list(path, limit)` and `archive::list_from_bytes(bytes)`, the latter
without a limit, a listing making nothing beyond its input; and `vdb::read_named(path,
unit, name, limit)` and `vdb::from_bytes_named(bytes, unit, name, limit)`, the name before
the limit, which stays last.

Issue 4 asked for a file written one grid at a time, so that a writer holds one grid's
memory, not every grid's. The owner decided, on 2026-11-10, as proposed, that the cache and
the archive each gain a `Writer` to a path: `Writer::create(path, count)`, then each entry
by `push` (and, for the archive, `push_with(entry, encoding)`), then `finish`, which
replaces the file whole as `write` does. The count comes first so that the directory's
room is known; the directory is written last, over zeros. An entry refused for itself is
not written and the writer goes on; after an error writing, every call is refused. Two
variants are added to `CacheError`: `EntryCount`, for an entry past the count or a finish
short of it, and `WriterFailed`. `write`, `write_with`, `to_bytes` and `to_bytes_with` are
made by the same code, their bytes unchanged.
