# Spike: value quantisation in the archive

Measured on 2026-10-29 with rustc 1.98.1 on an Apple M5, 10 threads, against brodo at
`be8721b`, each run started at rest (one-minute load below 2.5, five-minute below 4.0; the
loads are in `out/quantise-spike/*.load`). Reproduce from the repository's root:

```bash
cargo run --release --manifest-path spikes/quantise/Cargo.toml -- --flush > out/quantise-spike/flush-a.tsv
```

## Question

ROADMAP.md P6: how small, how wrong and how slow to read are an archive's leaf values
stored as IEEE binary16 or as 8 bits over a range, with and without byte shuffling? The
human set the criteria on 2026-10-29:

1. Every value's error is within its encoding's bound: binary16 at most 2⁻¹¹·|x| + 2⁻²⁵,
   8 bits at most (hi − lo)/510.
2. Decompressing and restoring take at most twice the yardstick, `CacheFile::read`'s copy
   and check without its file (spikes/archive/REPORT.md).
3. An encoding not smaller than exact `lz4_flex` is not adopted; shuffling is kept only
   where it shrinks.

Values beyond ±65504, and NaN, are refused at write; only leaf values are quantised, the
background and tiles stay exact.

## Method

Five frozen grids (docs/formats.md §2): SDFs of a sphere of radius 200 and of slabs 320 ×
320 × 6 and 640 × 640 × 6, from colino at h = 1 (`f32`, 33.8, 10.1 and 38.1 MB); a density
on the sphere's topology (`f32`, 33.8 MB); unit normals on a sphere of radius 100 (`[f64;
3]`, 50.0 MB). Each is cut as the archive cuts it (§8.3), in chunks of 512 leaves. A leaf
chunk's scalars are quantised, optionally shuffled by the quantised scalar's bytes, and
compressed by `lz4_flex` 0.14.0's block format; the other parts are compressed exact.
8 bits use each component's range, the grid's or each chunk's, two `f64` a component
counted in the size.

binary16 is converted by brodo's own code, round to nearest, ties to even; `--exhaustive`
checks it against both neighbours on every `f32` within ±65504. Reading decompresses the
chunks on rayon, unshuffles and restores through a table (65 536 entries for binary16, 256
a component for 8 bits, built once a process, outside the time) into one aligned buffer,
then checks it as a `GridView`. Every leaf value's error is measured against its bound,
and a run ends on one beyond it.

Timing (TESTING.md T5): 50 rounds, the yardstick and each encoding in turn in each round,
fastest and median kept. `--flush` rewrites 512 MB on every thread before each timed run,
outside the time, so that each starts with the grid out of the caches. Every configuration
ran twice.

## Results

The two runs of each configuration gave the same stored bytes and errors; their fastest
decode times agree within 7 %, and each median is within 17 % of its fastest.
`--exhaustive` found no `f32` within range converted wrong
(`out/quantise-spike/exhaustive.txt`).

Stored over raw bytes, and the largest error over its bound:

| Grid | Exact | f16 | f16 shuffled | u8 grid range | u8 chunk range | Error over bound |
| :--- | ---: | ---: | ---: | ---: | ---: | :--- |
| sdf sphere | 0.399 | 0.210 | 0.192 | 0.105 | 0.105 | 0.999 / 1.000 |
| sdf slab 320 | 0.523 | 0.247 | 0.190 | 0.064 | 0.064 | 0.999 / 1.000 |
| sdf slab 640 | 0.546 | 0.256 | 0.194 | 0.063 | 0.063 | 0.999 / 1.000 |
| density | 0.762 | 0.281 | 0.233 | 0.141 | 0.142 | 0.984 / 1.000 |
| normals | 0.947 | 0.250 | 0.150 | 0.068 | 0.090 | 0.993 / 1.000 |

Decompression and restoring over the yardstick, the fastest of each run, caches flushed
(`flush-a.tsv`, `flush-b.tsv`) and kept (`warm-a.tsv`, `warm-b.tsv`):

| Grid | Exact | f16 | f16 shuffled | u8 grid range | Yardstick |
| :--- | ---: | ---: | ---: | ---: | :--- |
| sdf sphere, flushed | 1.24 / 1.23 | 1.69 / 1.70 | 1.76 / 1.74 | 1.29 / 1.30 | 1.08 ms |
| sdf slab 320, flushed | 1.15 / 1.22 | 1.91 / 1.90 | 1.97 / 2.01 | 1.38 / 1.33 | 0.35 ms |
| sdf slab 640, flushed | 1.10 / 1.10 | 1.67 / 1.68 | 1.69 / 1.64 | 1.17 / 1.19 | 1.21 ms |
| density, flushed | 1.44 / 1.42 | 1.35 / 1.31 | 1.75 / 1.74 | 1.08 / 1.07 | 1.08 ms |
| normals, flushed | 1.14 / 1.14 | 0.97 / 0.97 | 1.10 / 1.10 | 0.91 / 0.91 | 1.58 ms |
| sdf slab 320, kept | 1.72 / 1.50 | 2.56 / 2.49 | 2.82 / 2.53 | 1.89 / 1.82 | 0.24 ms |
| sdf slab 640, kept | 1.09 / 1.09 | 1.78 / 1.76 | 1.76 / 1.76 | 1.21 / 1.20 | 1.11 ms |

With caches kept, the other grids are within a few percent of their flushed rows. Chunks
of 64 and 128 leaves (`c64-*.tsv`, `c128-*.tsv`) brought the small slab's binary16, caches
kept, to 2.1 to 2.4, and its 8 bits' ratio from 0.064 to 0.082.

Encoding a 33.8 MB grid takes 34 to 42 ms on 10 threads, whichever the encoding.

## Findings

1. Every encoding meets criterion 1, its largest error 0.98 to 1.00 of its bound, and
   criterion 3: the largest is 0.28 of raw bytes, against exact `lz4_flex`'s 0.40 to 0.95.
2. Shuffling shrinks binary16 by 8 to 40 % on every grid, and costs it at most 0.43 of the
   yardstick.
3. 8 bits over each chunk's range is never smaller than over the grid's, and is a third
   larger on the normals; the grid's range suffices.
4. Caches flushed, binary16 shuffled is at most 2.01 of the yardstick and 8 bits over the
   grid's range at most 1.38. Caches kept, only the 10 MB slab exceeds twice the yardstick,
   binary16 at 2.5 to 2.8: its yardstick drops from 0.35 ms to 0.24 ms, while the same
   shape four times larger stays at 1.8 either way. The small slab's excess is the copy's,
   held in the caches, not binary16's.
5. 8 bits' error is absolute: on an SDF it is 0.0118 of a voxel, and relative to values
   near zero it is unbounded (up to 9 × 10⁴ on the larger slab).

## What this leaves to decide (for the ADR)

- Whether the archive offers binary16 shuffled and 8 bits over the grid's range, each a
  value type's choice at write, on the reading of criterion 2 with caches flushed (the human
  chose it on 2026-10-29).
- How the encoding is recorded: in archive format v1, unreleased, or as a new version.
- How a range of 8 bits is stored, and what is refused: values beyond ±65504 or NaN for
  binary16, NaN or infinities for 8 bits.
