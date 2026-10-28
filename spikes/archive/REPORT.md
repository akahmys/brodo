# Spike: archive (B-005)

Measured on 2026-10-29 with rustc 1.98.1 on an Apple M5, against brodo at `c120a23`, twice,
at load averages of 2.41 and 2.87 (one minute). Reproduce from the repository's root:

```bash
cargo run --release --manifest-path spikes/archive/Cargo.toml > out/archive-spike/run1.tsv
```

`zstd` binds the C library, against CODING.md Rule 12: the human allowed it here as a
yardstick only (2026-10-28).

## Question

**B-005**: what is the archive profile's default compressor, and is it pure Rust (CODING.md
Rule 12)? The human set the criteria on 2026-10-28: pure Rust suffices if `ruzstd`'s ratio
is within 5 % of C zstd's at the same level, and if parallel decompression takes at most
twice `CacheFile::read`'s time; the default is the smallest codec meeting both.

## Method

Four frozen grids (docs/formats.md §2): an SDF of a sphere of radius 200 and one of a slab
320 × 320 × 6, from colino at h = 1 (`f32`, 33.8 MB and 10.1 MB); a density on the sphere's
topology (`f32`, 33.8 MB); unit normals on a sphere of radius 100 (`[f64; 3]`, 50.0 MB).
Each buffer is split into what comes before the leaves, chunks of 64, 512 or 4096 leaves,
and what follows. Every chunk is compressed on one thread, with and without byte shuffling
by scalar size, and decompressed by rayon on 10 threads into 32-byte blocks, then viewed
as a `GridView`; every result is asserted equal to the frozen bytes. The yardstick is the
same copy into blocks and the same view, without decompressing: `CacheFile::read` without
its file, so a codec within twice it is within twice `CacheFile::read`.

Codecs: `lz4_flex` 0.14.0 (block), `ruzstd` 0.9.1, `miniz_oxide` 0.9.1 (deflate 1, 6, 9)
and C `zstd` 0.14.3 (levels 1, 3, 7, 11). `ruzstd` 0.9.1 implements only its `Fastest`
level; `Default`, `Better` and `Best` panic `unimplemented!()`, so they were dropped.

## Results

Both runs gave the same compressed bytes; decompression times agree within 1 % at the
median and 10 % at most. Ratios are compressed over raw bytes, at 512 leaves a chunk,
unshuffled; times are the faster run's, over the yardstick.

| Codec | sdf sphere | sdf slab | density | normals | Decompression over yardstick |
| :--- | ---: | ---: | ---: | ---: | :--- |
| `lz4_flex` | 0.399 | 0.523 | 0.762 | 0.947 | 1.07 to 1.49 |
| `ruzstd` fastest | 0.436 | 0.550 | 0.594 | 0.836 | 4.5 to 11.2 |
| C zstd 1 | 0.334 | 0.471 | 0.724 | 0.827 | 2.1 to 3.9 |
| C zstd 11 | 0.311 | 0.469 | 0.386 | 0.352 | 2.0 to 4.0 |
| deflate 6 | 0.340 | 0.475 | 0.495 | 0.815 | 5.0 to 11.5 |

Compression on one thread of the 33.8 MB sphere: `lz4_flex` 18 ms, C zstd 1 26 ms,
`ruzstd` 150 ms, deflate 6 534 ms.

Shuffling shrinks C zstd 1's density to 0.527 and slab to 0.342, leaves its sphere alone,
and at level 11 more than doubles the normals (0.774); it slows `lz4_flex`'s decompression
2.2 to 3.6 times.

Chunks of 64 and 512 leaves differ by a few percent; 4096 leaves leave too few chunks for
10 threads and decompress 1.5 to 6 times slower. The raw rows are in
`out/archive-spike/run1.tsv` and `run2.tsv`.

## Findings

1. `ruzstd` fails both criteria: against C zstd 1 its ratio is 31 % larger on the sphere,
   17 % on the slab and 1 % on the normals, 18 % smaller on the density, and it
   decompresses 4.5 to 11 times slower than the yardstick.
2. Only `lz4_flex`, unshuffled, decompresses within twice the yardstick, on every grid.
   No codec, C zstd included, does so with shuffling.
3. C zstd at level 11 halves the density, and the normals by more, against `lz4_flex`, at
   2 to 4 times the yardstick; reaching it in pure Rust waits on `ruzstd`'s higher levels.
4. `lz4_flex`'s block API needs neither its frame format nor `twox-hash`; its default
   features decode with safe code and check bounds.

## What this leaves to decide (for the ADR)

- The default: `lz4_flex`, unshuffled, in chunks of 512 leaves, or another.
- Whether shuffling stays with quantisation only (ROADMAP.md P6), and whether a smaller
  codec is offered later.
