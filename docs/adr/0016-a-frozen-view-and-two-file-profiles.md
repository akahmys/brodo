# ADR-0016: A frozen grid is one aligned byte buffer, stored by two file profiles

- **Status**: Amended
- **Date**: 2026-10-16

Condensed on 2026-11-08 (ADR-0001): each decision is kept with its reason and what it set
aside; the text before is in git, this file as of `53f69d5`.

## Context

Grids are passed between processes and replayed from frame caches, where copying is the
cost; they are also archived and shipped, where size is. Compression and zero-copy
reading exclude each other. Future GPU use (WGSL) has no 8-bit loads and limited 64-bit
integers.

## Decision

The human decided, on 2026-10-16 unless dated otherwise. The bytes are docs/formats.md's;
version 2 of the layout and of both profiles replaced version 1's (ADR-0052, ADR-0054).

- **The frozen view.** `freeze()` packs tree and values into one contiguous buffer read as
  `GridView<'a>` without copying (`zerocopy`): 32-byte aligned, little-endian, in `u32`
  words, its sections placed by offsets in blocks and a link between nodes a number within
  its section (2026-10-27). Sections come in a set order, nodes in Morton order, and a
  node's mask numbers its children's places in Morton order, so that a child is found from
  its node's first child and a count of set bits, with no list of child numbers
  (2026-10-17). A bare frozen buffer is a file of its own, of one grid.
- **Two profiles.** The **cache** profile stores the buffer as is, read by memory mapping.
  The **archive** profile stores it in compressed chunks, decompressed in parallel into a
  `GridView` with one copy. Every file carries a magic number and a format version, and
  per grid its name, value type, voxel size and unit, transform, bounding box and free
  key-value metadata; physical size is always stored.
- **Value types** are bounded by `zerocopy`'s traits, `Copy`, `Send` and `Sync`; they
  began as `f32`, `f64`, `u8`, `u32` and `[f32; 3]` (ADR-0029, ADR-0056).
- **Metadata** (2026-10-17) is a `GridMeta` of its own, so that a solver writing
  `Topology` and `Field<T>` directly writes files as a single-field user does; the unit is
  a string brodo does not interpret, units being its users' (AGENTS.md S4).
- **Versions** (2026-10-17). A reader accepts its own versions, any other a typed error
  naming it (B-009); which versions a release reads is TESTING.md V6's.
- **A reader is strict** (2026-10-18). Each reader parses untrusted bytes, is listed in
  `UNTRUSTED_INPUT` and fuzzed (ADR-0009), and accepts only the bytes its writer puts for
  what it read: parts in their places, nothing after the last, unused words and padding
  zero, a grid's metadata equal to its re-encoding (a key twice or out of order refused).
  A review found 64 bytes appended, a gap, a key twice and non-zero padding accepted.
  What a reader still leaves unchecked docs/formats.md states.
- **The archive's compressor** (2026-10-29, B-005). Each chunk is compressed with
  `lz4_flex`'s block format, without its `frame` feature, so that `twox-hash` is not
  pulled in; 512 leaves a chunk. `spikes/archive/`: `ruzstd` 0.9.1, the pure-Rust zstd,
  had only its fastest level, compressed SDFs 17 to 31 % larger than C zstd 1 and
  decompressed 4.5 to 11 times slower than a plain copy; only `lz4_flex` stayed within
  twice the copy on every grid. A smaller codec waits until `ruzstd` has its higher
  levels (ROADMAP.md §4). Smaller chunks for a region's read saved at most 0.35 ms while
  costing up to 9 % (128 leaves) and 28 % (64) in size.
- **The archive's file** (2026-10-29). An archive is read back as a `CacheFile`, its
  chunks decompressed into the bytes the cache profile would hold and checked as a cache
  file is, so that one reader's checks and one set of views serve both. Neither profile
  carries a checksum: a reader checks structure, and a flipped bit in a value is left to
  the transport. A reader does not check that a chunk's compressed bytes are those the
  writer's compressor would give, which would cost a compression per read and refuse
  files of an older `lz4_flex`. It is built under the `archive` feature (ADR-0036), so that
  `lz4_flex` reaches only a user who asks for it.
- **Quantised values** (2026-10-29, `spikes/quantise/`). A grid's leaf values may be
  stored exact, as IEEE 754 binary16 rounded to nearest, ties to even, byte-shuffled, or
  as 8 bits over each component's range across the grid's leaf values, the writer
  choosing per grid; only floating types, the background and tiles exact. The writer
  refuses a value its encoding cannot hold, naming the grid. Encodings were adopted only
  if every value kept its bound, they were smaller than exact `lz4_flex`, and reading cost
  at most twice a cache read with the caches flushed, as a file is read; byte shuffling
  shrank binary16 by 8 to 40 %, and 8 bits over each chunk's range was never smaller than
  over the grid's. A reader refuses an encoding code it does not know.
- **A region of an archive** (2026-10-29). `archive::Region` holds one grid's first chunk,
  its tile chunk and the leaf chunks meeting a box of voxels, each restored as a whole read
  does; its `RegionView` answers every voxel within the box and refuses one outside it
  (`OutsideRegion`) even where its chunks cover it, so that what it answers does not follow
  from how the grid is cut. It checks the structure as a whole read does and the chunks it
  decompresses, not the others. A view over a grid-sized buffer was set aside: zeroing
  that buffer alone made it only 1.2 to 4 times faster than a whole read. The file is
  still read whole; reading only the chunks' bytes is left to later (ROADMAP.md §2).
- **A listing, and grids by name** (2026-11-09 and 2026-11-10, issue 2). An archive is
  listed from its directory and metadata, decompressing no chunk, its structure and every
  entry's metadata checked as a region's read checks its grid's, so that one grid of a
  large file is picked cheaply and then read alone as the region of its bounds; the
  listing gives its entries of bytes too, as a `CacheFile` does, their metadata being
  checked anyway. Grids are found by name in a cache file, an archive and a `.vdb`, every
  match in the file's order, since names may repeat and brodo does not choose among them;
  a `.vdb` read for one name skips each other grid by its offsets, unread. The file is
  still read whole, as for a region.
- **A whole read's buffer** (2026-10-30) is written with zeros rather than taken zeroed
  from the allocator: the saving was about 1 % of a read, and `zerocopy`'s `alloc` feature
  and a failure of its own to handle would come with it.
- **`.vdb` files** are read and written by brodo itself (ADR-0045, ADR-0046, ADR-0047);
  this record's first decision, to read them through `vdb-rs`, was reversed when `vdb-rs`
  was found pulling C.

## Consequences

The layout and formats are versioned apart from the API (TESTING.md V6). A region's read
costs a whole file's read until the chunks' bytes are read alone.
