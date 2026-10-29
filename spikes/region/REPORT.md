# Spike: reading a region of an archive

Measured on 2026-10-29 with rustc 1.98.1 on an Apple M5, 10 threads, against brodo at
`7275064`, each run started at rest (one-minute load below 2.5, five-minute below 4.0; the
loads are in `out/region-spike/*.load`). Reproduce from `spikes/region/`:

```bash
cargo run --release -- --big > ../../out/region-spike/run-b.tsv
```

## Question

ROADMAP.md P6: how much of a grid does a box of voxels need decompressed, and how much
faster is reading only that than the whole grid? The human chose on 2026-10-29 a view that
answers within what it read, holding only the chunks it needs, each in its own buffer, with
no buffer the size of the grid; and asked that 64, 128 and 512 leaves a chunk be compared.

## Method

Four frozen grids (docs/formats.md §2), all `f32`: SDFs of a sphere of radius 200 and of a
slab 640 × 640 × 6, from colino at h = 1 (16 307 and 18 520 leaves, 33.8 and 38.1 MB); a
density on every leaf of cubes of edge 256 and 512 (32 768 and 262 144 leaves, 67.1 and
536.9 MB). Each is cut as the archive cuts it (§8.3), with 64, 128 or 512 leaves a chunk,
each chunk compressed exact by `lz4_flex` 0.14.0's block format.

A box whose side is 0.1, 0.2 or 0.5 of the grid's leaf bounds along each axis is placed at
the low end, the middle and the high end of each axis, 27 places. A leaf chunk is needed
when one of its leaves meets the box; the first chunk (header and nodes) and the tile chunk
always are. Reading a region decompresses the needed chunks on rayon, each into a buffer of
its own; reading the whole decompresses every chunk into one zeroed aligned buffer and
checks it as a `GridView`, as the archive's reader does. Both are checked against the
frozen bytes outside the time.

Timing (TESTING.md T5), the box at the place needing the most leaf chunks: 30 rounds, a
region and the whole in turn, each after 512 MB are rewritten on every thread outside the
time, fastest and median kept. `--parts` times the whole read's fixed costs. Run three
times (`run-a`, `run-b`, `run-c`).

## Results

The bytes held are the same in every run. `run-b` and `run-c` agree within 15 % on every
time, most within 10 %; `run-a`, though started at rest, is 1.3 to 2.5 times slower on
both reads and is left out. Times below are `run-b`'s fastest.

Share of the grid held for a box, mean / largest over its 27 places:

| Grid | Side | 64 leaves | 128 leaves | 512 leaves |
| :--- | ---: | ---: | ---: | ---: |
| sdf sphere | 0.1 | 0.006 / 0.027 | 0.009 / 0.052 | 0.029 / 0.133 |
| sdf sphere | 0.5 | 0.150 / 0.179 | 0.158 / 0.191 | 0.209 / 0.287 |
| sdf slab | 0.2 | 0.016 / 0.119 | 0.023 / 0.162 | 0.050 / 0.280 |
| sdf slab | 0.5 | 0.204 / 0.499 | 0.216 / 0.534 | 0.271 / 0.725 |
| density 512 | 0.1 | 0.002 / 0.002 | 0.003 / 0.004 | 0.005 / 0.015 |
| density 512 | 0.5 | 0.125 / 0.131 | 0.125 / 0.131 | 0.125 / 0.131 |

Reading the box needing the most, against reading the whole:

| Grid | Side | Whole | 64 leaves | 128 leaves | 512 leaves |
| :--- | ---: | ---: | ---: | ---: | ---: |
| sdf sphere | 0.1 | 1.20 ms | 0.18 ms | 0.17 ms | 0.29 ms |
| sdf sphere | 0.5 | 1.21 ms | 0.29 ms | 0.26 ms | 0.44 ms |
| sdf slab | 0.2 | 1.12 ms | 0.15 ms | 0.20 ms | 0.37 ms |
| sdf slab | 0.5 | 1.13 ms | 0.43 ms | 0.48 ms | 0.84 ms |
| density 256 | 0.1 | 2.75 ms | 0.08 ms | 0.14 ms | 0.47 ms |
| density 512 | 0.1 | 19.4 ms | 0.10 ms | 0.15 ms | 0.44 ms |
| density 512 | 0.5 | 18.4 ms | 1.77 ms | 1.94 ms | 2.27 ms |

The whole read's zeroed buffer alone takes 0.24 ms for the sphere and 3.7 to 4.1 ms for the
537 MB density (`parts-b.tsv`, `parts-c.tsv`); its `GridView` check under 0.01 ms.

spikes/quantise/ measured what the chunk size costs in size (`c64-a.tsv`, `c128-a.tsv`,
`c512-a.tsv` in `out/quantise-spike/`): exact values within 1.5 %, 8 bits over the grid's
range on the slab 0.0638 of raw bytes at 512 leaves, 0.0693 at 128, 0.0818 at 64.

## Findings

1. Reading a region holds the region's share of the grid, rounded up to chunks, and its time
   follows the bytes held plus 0.1 to 0.3 ms fixed: 0.02 of the whole read for a box of side
   0.1 on the 537 MB grid at 512 leaves, 0.13 for side 0.5.
2. Smaller chunks shrink what a small box holds by up to a factor of 7, but save at most
   0.35 ms a read; for a box of side 0.5 they save little. They cost size that stays in the
   file: up to 9 % at 128 leaves and 28 % at 64 for 8 bits.
3. The whole read cannot approach a region's: its zeroed buffer alone costs more than most
   region reads.

## What this leaves to decide (for the ADR)

- The chunk size: 512 leaves, the header's value, or smaller.
- The view's API, and what it answers outside its box.
- What reading a region checks, and what the chunks it does not read leave unchecked.
