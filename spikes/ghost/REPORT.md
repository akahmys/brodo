# Spike: the ghost layer's width (B-002), and gathering several fields at once

Measured on 2026-10-20 on an Apple M5 at rest (load average 1.33 before, 1.93 after), with
brodo at 98b6f34, on brodo's benchmark harness (ADR-0019): two fresh processes, 20 rounds
each, the fastest time kept; every pair agreed within 4.2 %. Times below are the faster of
the pair, from `out/bench/ghost/report.tsv`. Every run is on one thread, leaf after leaf.

```bash
cd spikes/ghost && cargo build --release
./target/release/ghost-spike check   # agreement, and reads beyond each window
./target/release/ghost-spike         # timed, at rest
```

## Questions

- **B-002**, asked by a user as R9 (ROADMAP.md §4): a stencil reading up to three voxels
  away. Is a gather wider than one layer worth having, against brodo's one-layer gather
  with reads beyond it through `ValueAccessor` or `Field::value_at`?
- **R10**: does gathering several fields of one leaf with its neighbours looked up once pay?

Decided before measuring: a gather three layers wide is proposed if it is 10 % or more
faster than one or two layers on some grid; sharing the lookups is proposed only if it
saves 10 % or more at three fields. 10 % is the harness's own tolerance.

## Workload

Each leaf voxel reads 19 `f64` values: itself and ±1, ±2, ±3 along each axis. Grids: a
sphere's narrow band (radius 80, band ±3, 2552 leaves, 3064 inside tiles); a block of 16³
leaves (4096); a slab one leaf thick, 32 × 32 leaves (1024, 256² voxels), as a
two-dimensional run is kept.

Variants: `w1_value_at` (brodo's gather, `value_at` beyond it, as the user did);
`w1_accessor` (an accessor beyond it); `w2_accessor` (a gather two layers wide, 12³, an
accessor beyond); `w3` (three layers, 14³, every read from it); `accessor` (no gather).
The wider gathers copy brodo's method: each window voxel from the block of the 27 around
the leaf holding it.

## Correctness

All five variants' sums agree bit for bit on every grid, and the spike's gather one layer
wide equals brodo's `gather` voxel for voxel; shifting one variant's reads by a voxel made
the sums disagree on all three grids. 11.8 % of the reads fall beyond one layer and 3.9 %
beyond two, on every grid, since every leaf voxel reads the whole stencil (the user
counted 15.6 % on its own grid and stencil).

## R9: reading three voxels away

| Grid | `w1_value_at` | `w1_accessor` | `w2_accessor` | `w3` | `accessor` | `w3` against `w1_accessor` | against `w2` |
| :--- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| sphere | 137.5 ms | 49.6 ms | 32.9 ms | 27.5 ms | 114.1 ms | −44.7 % | −16.4 % |
| block | 75.5 ms | 67.0 ms | 47.8 ms | 43.8 ms | 160.2 ms | −34.6 % | −8.3 % |
| slab | 15.1 ms | 14.6 ms | 11.2 ms | 10.8 ms | 35.3 ms | −25.6 % | −3.3 % |

A gather three layers wide is the fastest on every grid, and more than 10 % faster than
one layer everywhere and than two on the sphere: the criterion is met. Reading everything
through an accessor is the slowest, 3.3 to 4.2 times `w3`. `value_at` beyond one layer
costs 2.8 times an accessor on the sphere, where neighbours are often tiles, and about the
same on the block and slab. The slab gains least, since its reads along z beyond the leaf
meet the background whatever the width.

## R10: several fields of one leaf

| Grid | k | apart | shared | change | lookups alone, share of apart |
| :--- | ---: | ---: | ---: | ---: | ---: |
| sphere | 1 / 3 / 6 | 3.42 / 10.39 / 20.73 ms | 3.51 / 10.28 / 20.96 ms | +2.7 / −1.1 / +1.1 % | 2.9 / 2.2 / 1.7 % |
| block | 1 / 3 / 6 | 5.35 / 16.30 / 33.59 ms | 5.23 / 15.82 / 33.20 ms | −2.3 / −2.9 / −1.2 % | 1.4 / 1.3 / 1.3 % |
| slab | 1 / 3 / 6 | 1.27 / 3.85 / 7.67 ms | 1.28 / 3.86 / 7.88 ms | +0.9 / +0.5 / +2.8 % | 1.2 / 1.2 / 1.2 % |

Looking the neighbours up takes 1.2 to 2.9 % of a gather; sharing it changed the time by
−2.9 to +2.8 %, within the pairs' spread. The criterion is not met. A gather's time is in
its copy, voxel by voxel, each through a division and a match; that is where any speed
would come from, for one field or several.

## Not measured

Several threads (`par_update`); `f32` fields; stencils other than the axial one; a gather
wider than three layers (a leaf of 8³ reaches only its 26 neighbours up to seven); a copy
by rows rather than voxel by voxel.
