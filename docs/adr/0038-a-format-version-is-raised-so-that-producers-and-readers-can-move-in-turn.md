# ADR-0038: A format version is raised so that producers and readers can move in turn

- **Status**: Amended
- **Date**: 2026-10-25

## Context

Each layout and file profile carries its own format version (TESTING.md V6), and every
reader accepts its own version only, refusing another with an error naming it (ADR-0016,
ADR-0032). Some users pass files across process boundaries, a producer and a reader
possibly built against different releases. When a version is raised, a reader of the old
one refuses every new file, and a reader of the new one every old file.

## Decision

The owner chose, on 2026-10-25, the recommended option, (1):

- (1) **Read the current version and the one before it.** A release that raises a version
  still reads the previous one, and can write it on request; the version before that is
  refused, naming it. Producers and readers can then upgrade in either order across one
  release.
- (2) **Read every version ever published.** No file ever becomes unreadable; each old
  reader stays in the code and in the fuzzer for good.
- (3) **Read the current version only, as now**, the error naming the file's version and
  the one expected; a converter in the crate rewrites an old file to the new version.
- (4) **As now, with nothing more.**

*Recommended: (1)*: it gives every pair of processes one release to move in, at the cost
of one previous reader at a time, each kept fuzzed and tested on its golden file.

## Consequences

Whatever is chosen is stated in the format document (ADR-0037) and in TESTING.md V6, and
held by a Format-tier test reading each version the policy keeps.

## Amendment: a Hermite set's shared nodes

- **Date**: 2026-10-27

P5's review found that version 1's reader leaves nodes shared between entries unchecked
(docs/formats.md §2.5): two root entries naming one upper node, or nodes whose children's
or tiles' ranges overlap. A reader walking the whole tree, as checking a Hermite set does,
then walks the shared nodes once per entry: a file of a few megabytes asked for tens of
gigabytes, and a leaf reached twice was checked at one of its origins only. B-022 had left
such checks for a version raised for another reason. The human decided that a Hermite set
is refused when the walk from its sign grid's root table reaches a node, a leaf or a tile
value twice (§4.2, rule 11, `FormatError::Shared`), at version 1: brodo writes no such
set, and waiting would leave the reader to such files. A frozen grid read voxel by voxel,
whose cost sharing does not raise, keeps version 1's checks.

