# ADR-0016: A frozen grid is one aligned byte buffer, stored by two file profiles

- **Status**: Amended
- **Date**: 2026-10-16

## Context

Grids are passed between processes and replayed from frame caches,
where copying is the cost; they are also archived and shipped, where size is. Compression
and zero-copy reading exclude each other. Future GPU use (WGSL) has no 8-bit loads and
limited 64-bit integers.

## Decision

- `freeze()` packs tree and values into one contiguous buffer read as `GridView<'a>`
  without copying (`zerocopy`). Links are offsets in 32-byte units; every structure is
  32-byte aligned, little-endian, and laid out in `u32` words.
- The **cache** profile stores that buffer as is, read by memory mapping. The **archive**
  profile stores metadata, topology and value blocks in compressed chunks of hundreds to
  thousands of leaves, decompressed in parallel into a `GridView` with one copy, with
  optional quantisation followed by byte shuffling.
- Every file carries a magic number, a format version, the grid count, and per grid its
  name, value type, voxel size and unit, transform, bounding box and free key-value
  metadata. Physical size is always stored.
- OpenVDB files are read through the `vdb-rs` crate, and written by brodo from the
  published format documentation, after P1 and P2.
- Value types start with `f32`, `f64`, `u8`, `u32` and `[f32; 3]`, bounded by `zerocopy`'s
  traits, `Copy`, `Send` and `Sync`.

## Consequences

The layout and formats are versioned apart from the API (TESTING.md V6). The archive's
default compressor is open (ROADMAP.md §3, B-005).

## Amendment

- **Date**: 2026-10-17

Before `freeze` was built, the human decided three points. B-009: a reader accepts only
its own layout version; any other is a typed error naming the version, and the question
is opened again when a second version exists. The work was split into the frozen view,
then the cache file with its metadata, then memory mapping. And the layout of version 1
was fixed in outline (ARCHITECTURE.md §4.6): sections in a set order, nodes in Morton
order, and node masks numbering children's places in Morton order, so that a child is
found from its node's first child and a count of set bits, with no list of child numbers.
The view's reader parses untrusted bytes, so it was listed in `UNTRUSTED_INPUT` and fuzzed
(ADR-0009).

## Amendment

- **Date**: 2026-10-17 (second)

Before the cache file was built, the human decided that a grid's metadata is a separate
`GridMeta`, so that a solver writing `Topology` and `Field<T>` directly can write files as
a single-field user can, and that the unit is a string brodo does not interpret, since
units belong to its users (AGENTS.md S4). The cache profile's format version 1 was laid
out in blocks, directory first, each grid's frozen buffer on a block boundary, and a
reader accepts its own format version only, as for the frozen layout. The file reader
parses untrusted bytes, so it was listed in `UNTRUSTED_INPUT` and fuzzed (ADR-0009).

## Amendment

- **Date**: 2026-10-18

A review found that the cache reader, though it "checks the whole file", accepted bytes
the writer never puts: 64 bytes appended after the last grid, a gap before a part, a key
twice in a grid's free metadata (the second value kept) and non-zero padding. The human
decided that a reader accepts only the bytes the writer puts for what it read: each part
at the block after the one before, nothing after the last grid, the header's and the
directory's unused words zero, padding zero, and a grid's metadata equal to its
re-encoding, which also refuses a key twice or out of order and an empty box's non-zero
corners. Any other file is `CacheError::Inconsistent`, naming the part. The bytes written
did not change, so the format version stayed 1 (TESTING.md V6); only files brodo did not
write are refused.
