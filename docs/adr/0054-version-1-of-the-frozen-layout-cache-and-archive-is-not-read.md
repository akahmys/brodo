# ADR-0054: Version 1 of the frozen layout, the cache and the archive is not read

- **Status**: Accepted
- **Date**: 2026-11-05

## Context

0.9.0 wrote version 2 of the frozen layout and the cache and archive profiles, read
version 1 of each, and wrote it on request (ADR-0051, ADR-0052), as TESTING.md V6 asked.
That left three pairs of writers (`cache::to_bytes_version`, `cache::write_version`,
`archive::to_bytes_version`, `archive::write_version`, `FrozenGrid::with_version`), two
errors of version 1 alone (`FreezeError::Version`, `CacheError::BytesAtVersion1`), a
reader branching on two versions, and a second check, the walk docs/formats.md then stated refusing
a node reached twice (`FormatError::Shared`), which version 2's writer rules 9 and 11 make
needless. A survey of designs grown by accretion before 1.0 (ROADMAP.md P9) found these,
and after 1.0 V6 would have them carried for good. ADR-0038's amendment lets the owner set
V6 aside before 1.0, and ADR-0053 did so for Hermite data and the SDF convention.

## Decision

The owner decided, on 2026-11-05, that version 1 of the frozen layout, the cache profile
and the archive profile be neither read nor written, and that the walk's check go with it:

- The writers of version 1 and their errors are gone; `to_bytes`, `write`, `to_bytes_with`
  and `write_with` write version 2, and a reader refuses version 1 with
  `UnsupportedVersion(1)`.
- `FormatError::Shared` is gone: a view is made only once rules 1 to 11 of
  docs/formats.md §2.5 hold, which give each node, leaf and tile value one place, so a
  walk, whole or of a region, reaches each once.
- docs/formats.md states each format at its version alone (§2, §3, §8); the golden files
  of version 1 are gone, `hermite_v1.bin` among them, a cache file of version 1, and
  `archive_v2_grids.bin` and `archive_v2_quantised.bin` take the place of version 1's
  archives; `hermite_v2.bin` stays, the set ADR-0053 refuses.
- When a version is raised after 1.0, writing the one before on request comes back as V6
  asks, designed once for the three formats.

## Consequences

A file written by 0.8 or earlier, or by 0.9.0 on request at version 1, is not read until
rewritten: read by 0.9.0, which reads it, and written again by 0.9.0, which writes
version 2, which this release reads. `FrozenGrid::version`, `GridView::version` and
`CacheFile::version` stay, answering 2. The mutation run at P9's close is the first of
the code without version 1; an exclusion it finds equivalent is recorded then. The mutant
excluded in `pack`'s restamping of a frozen buffer to another version (ADR-0023) went with
it.
