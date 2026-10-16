# ADR-0016: A frozen grid is one aligned byte buffer, stored by two file profiles

- **Status**: Accepted
- **Date**: 2026-10-16

## Context

Grids are passed between processes (the solver and the design platform) and replayed from frame caches,
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
