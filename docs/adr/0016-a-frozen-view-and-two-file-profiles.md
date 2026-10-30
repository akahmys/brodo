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

## Amendment: what the design says the readers check

- **Date**: 2026-10-27

P5's review found three statements of this record the formats outgrew. "Links are offsets
in 32-byte units": offsets in blocks place a buffer's sections, and a link between nodes is
a number within its section. "Every file carries … the grid count": a bare frozen buffer is
a file of its own too, with one grid and no count (docs/formats.md §2). And the 2026-10-18
amendment's reader "accepts only the bytes the writer puts" held for the cache file's own
parts, but a grid's buffer is checked as docs/formats.md §2.5 says, which leaves its
padding, the order of its nodes and some of its writer's rules unchecked, B-022 keeping
them for a later version (ADR-0038). ARCHITECTURE.md §4.6 and §4.7 state each as it is.

## Amendment: the archive's compressor

- **Date**: 2026-10-29

B-005 asked for the archive's default compressor and whether it is pure Rust.
`spikes/archive/` compressed four frozen grids in chunks and decompressed them in parallel
into a `GridView` (its REPORT.md). `ruzstd` 0.9.1, the pure-Rust zstd, implemented only its
fastest level; against C zstd 1 it compressed the SDFs 17 to 31 % larger, and it
decompressed 4.5 to 11 times slower than a plain copy into a `GridView`. Only `lz4_flex`
decompressed within twice that copy, on every grid. The human decided:

- Each chunk is compressed with `lz4_flex` 0.14's block format, unshuffled, by default 512
  leaves a chunk. `lz4_flex` was approved as a dependency (CODING.md Rule 20), without its
  `frame` feature, so that `twox-hash` is not pulled in.
- Byte shuffling comes only with quantisation (ROADMAP.md P6).
- No smaller codec is offered for now; when `ruzstd` implements its higher levels, it is
  measured again (ROADMAP.md §4).

## Amendment: the archive profile's file

- **Date**: 2026-10-29 (second)

Before the archive profile was built, the human decided four points:

- An archive is read back as a `CacheFile`: its chunks are decompressed, in parallel, into
  the bytes the cache profile would hold for the same grids, and those are checked as a
  cache file is, so that one reader's checks and one set of views serve both profiles.
- It carries no checksum, as the cache file carries none: a reader checks structure, and
  a flipped bit in a value goes unseen, left to the transport; a later version may add one.
- A reader checks what the writer puts down to how each grid is cut into chunks, their
  places and their padding, but not that a chunk's compressed bytes are the ones the
  writer's compressor would give, which would cost a compression per read and refuse files
  of an older `lz4_flex`; docs/formats.md states the gap.
- It is built under the `archive` feature, opted into (ADR-0036), so that `lz4_flex` reaches
  only a user who asks for it.

## Amendment: quantised values in the archive

- **Date**: 2026-10-29 (third)

ROADMAP.md P6 asked for an archive's values quantised to 16-bit floats and to 8 bits over
a range, with byte shuffling. `spikes/quantise/` measured five frozen grids (its REPORT.md)
against criteria the human set: every value within its encoding's error bound, reading at
most twice `CacheFile::read`'s copy and check, and an encoding adopted only if smaller than
exact `lz4_flex`. Every encoding met the first and third; byte shuffling shrank binary16 by
8 to 40 %, and 8 bits over each chunk's range was never smaller than over the grid's. On
reading, a 10 MB grid exceeded twice the yardstick, binary16 at 2.5 to 2.8, only while its
bytes stayed in the caches between runs, which sped the copy, not binary16: with the caches
flushed before each run it was 1.9 to 2.0, and the same shape four times larger 1.7
either way. The human decided:

- The criterion on reading is measured with the caches flushed before each run, as a file
  is read.
- A grid's leaf values may be stored exact, as IEEE 754 binary16 rounded to nearest, ties
  to even, byte-shuffled, or as 8 bits over each component's range across the grid's leaf
  values, the writer choosing per grid. Only floating value types are quantised; the
  background and the tiles stay exact. The writer refuses, naming the grid, a value
  binary16 cannot hold (NaN, or beyond ±65504) and one 8 bits cannot (NaN, infinite, or a
  range wider than an `f64` holds).
- The encoding is a code in the directory entry's first unused word, 0 for exact, and a
  reader that does not know a code refuses it, as with a value type code: archive format
  version 1 is extended, not raised, its exact files' bytes unchanged. The human weighed
  raising the version, or redrawing version 1, unreleased, and found that a design drawn
  anew places the code and the ranges where these do.
- An 8-bit grid's ranges, two `f64` a component, follow its chunk table, within its part.
  A chunk table still states each chunk's length in the frozen buffer; a quantised chunk
  decompresses to its encoded length, which follows from that.
- A reader restores the values and checks the cache file so made as before; it checks an
  encoding's code against the value type, and the ranges as finite and ordered, but not
  that the codes are ones the writer gives.

## Amendment: reading a region of an archive

- **Date**: 2026-10-29 (fourth)

The human added to ROADMAP.md P6 a reader of one grid's region that decompresses only the
chunks a box of voxels needs, and chose a view that answers within what it read, holding
those chunks each in a buffer of its own, with none the size of the grid, over one that
decompresses into a grid-sized buffer, which the zeroing of that buffer alone held to 1.2
to 4 times faster than a whole read. `spikes/region/` measured four grids, 64, 128 and
512 leaves a chunk (its REPORT.md): a region's read follows the bytes it holds, 0.02 of a
whole read for a box a tenth of a 537 MB grid's side at 512 leaves, and smaller chunks
saved at most 0.35 ms a read while costing up to 9 % (128) and 28 % (64) in size for 8
bits. The human decided:

- Chunks stay 512 leaves: the header's value is unchanged.
- A region is read as an owned `archive::Region` of one grid and a box of voxels, holding
  the grid's first chunk, its tile chunk and the leaf chunks holding a leaf that meets the
  box, each decompressed and restored as a whole read does; its `RegionView` answers every
  voxel within the box, and refuses one outside it as `OutsideRegion`, even where the
  chunks it holds cover it, so that what it answers does not follow from how the grid is
  cut.
- A region's read checks the archive's structure as a whole read does, its first chunk's
  frozen header and nodes as a `GridView` does, and the chunks it decompresses; a chunk it
  does not decompress is not checked to decompress, nor its values. docs/formats.md states
  the gap; a whole read checks everything.
- The file is read whole, as a whole read reads it; reading only the bytes of the chunks
  needed is left to later.

## Amendment: the whole read's buffer

- **Date**: 2026-10-30

Reading a region left to P6's benchmarks whether a whole read makes its grid-sized buffer
with `zerocopy`'s `new_vec_zeroed`, the allocator's zeroed memory, rather than writing
zeros over it. `spikes/region/` had timed both on its 537 MB grid (`parts-b.tsv`): 4.09 ms
zeroed, 4.32 ms written, 0.2 ms of a whole read's 18 to 19 ms. The owner decided that the
buffer stays written: the saving is about 1 %, and `new_vec_zeroed` would need `zerocopy`'s
`alloc` feature and a failure of its own to handle.
