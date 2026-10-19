# Spike: dilate (B-011)

Measured on 2026-10-19 with rustc 1.98.1 (release), on an Apple M5 (10 cores, 32 GB).
Sizes and results do not depend on load; times were taken at a one-minute load average
of 4.4 to 4.9, so they are rough. Reproduce from this directory:

```bash
cargo build --release
./target/release/dilate-spike check                          # the bitset against hand counts
./target/release/dilate-spike sizes > ../../out/dilate/sizes.tsv
./target/release/dilate-spike time 5 > ../../out/dilate/time-1.tsv
ONLY="sphere R=80" ./target/release/dilate-spike sizes       # one shape
```

## Question

**B-011**: do dilation and erosion of a topology need a per-voxel active mask in each
leaf? Today a leaf holds none: all its voxels are active (ADR-0013).

## Candidates

| | Active set | Dilation by k | Erosion by k |
| :--- | :--- | :--- | :--- |
| A, no mask | every voxel of every leaf and tile | adds every background leaf-sized cell with a voxel within k steps of an active cell | removes every leaf with a background voxel within k steps |
| B, a 512-bit mask per leaf | the band's voxels and the tiles' | exact, voxel by voxel | exact |

Steps are 6- or 26-connected, k from 1 to 8. A uses brodo's own edits: activating cells,
or rebuilding without the removed ones, since brodo cannot remove a leaf yet. B is
computed on a dense bitset: its result and size, not its time.

## Inputs

Narrow bands of half-width 3 voxels, leaves where a voxel has |φ| < 3, lower tiles where
every voxel has φ ≤ −3: spheres of radius 80, 250 and 500; a torus (R 120, r 40); a plate
128 × 128 × 1, thinner than a voxel, so that no voxel has φ < 0.

| Shape | Leaves | Leaf voxels | Band voxels | Leaf / band |
| :--- | -: | -: | -: | -: |
| sphere R=80 | 2552 | 1306624 | 482840 | 2.71 |
| sphere R=250 | 25528 | 13070336 | 4712448 | 2.77 |
| sphere R=500 | 101368 | 51900416 | 18848544 | 2.75 |
| torus | 6136 | 3141632 | 1137128 | 2.76 |
| plate | 648 | 331776 | 107736 | 3.08 |

## Measurements

**A is sound** in all 80 cases (5 shapes × 2 connectivities × k in {1, 2, 3, 8} × dilate
and erode): its dilation holds every voxel of the exact dilation of its own active set,
and its erosion only voxels of the exact erosion. Shown to catch a fault: with the cells
near a cell cut short (`steps < k`), the check failed exactly in the cases the cut
changed (6-connected k ≤ 3, 26-connected k = 1). The bitset's morphology matched hand
counts across 64-bit word boundaries (`check`).

**A is coarse.** With every voxel of a leaf active, the cells within reach of a step are
the whole ring: 26-connected k = 1 already gives what k = 8 does. For the sphere R=500:

| Op, conn, k | A leaves | A MiB | B leaves | B MiB | Exact on A's set, voxels | A's voxels |
| :--- | -: | -: | -: | -: | -: | -: |
| dilate, 26, 1 | 178704 | 349.0 | 110224 | 222.0 | 56764744 | 91496448 |
| dilate, 26, 3 | 178704 | 349.0 | 130312 | 262.5 | 66566168 | 91496448 |
| dilate, 26, 8 | 178704 | 349.0 | 178704 | 359.9 | 91496448 | 91496448 |
| dilate, 6, 1 | 144040 | 281.3 | 106288 | 214.1 | 56416592 | 73748480 |
| erode, 26, 1 | 27056 | 52.8 | 82880 | 166.9 | 47060280 | 13852672 |
| erode, 6, 1 | 59576 | 116.4 | 91504 | 184.3 | 47395704 | 30502912 |

A's memory over B's after dilating (values as `f32`, 2048 B a leaf; B 64 B more):

| k | sphere R=500, 6 / 26 | torus, 6 / 26 | plate, 6 / 26 |
| -: | :-: | :-: | :-: |
| 1 | 1.31 / 1.57 | 1.42 / 1.72 | 2.15 / 2.38 |
| 2 | 1.46 / 1.44 | 1.59 / 1.59 | 2.38 / 2.38 |
| 3 | 1.48 / 1.33 | 1.62 / 1.46 | 2.38 / 2.38 |
| 8 | 1.20 / 0.97 | 1.22 / 0.97 | 1.07 / 0.97 |

Erosion by A keeps 29 % (26-connected) to 64 % (6-connected) of the exact erosion of its
own set at k = 1 on the sphere R=500, since a leaf goes whole once a background voxel is
within reach of any of its voxels.

**Downstream**: dilating by A changed no solid's Euler characteristic (sphere 1, torus 0,
plate 0, under both connectivities), since the new leaves hold the background's value.

**A's time**, fastest of 5, two processes (rough, see above): dilation 7 to 13 ms for the
sphere R=80, 118 to 386 ms for R=250, 0.7 to 1.9 s for R=500, most of it brodo's commit
of the new leaves; erosion by rebuilding 2 to 3 ms, 36 to 57 ms and 0.2 to 0.3 s.

## Answer

A needs nothing new in a leaf and stays sound, so every reader of a leaf (gather,
labelling, χ, the frozen layout and cache format, VTK) is unchanged; B would add a mask
every one of them must honour, and a format version. A's price is coarseness: after
dilating by k ≤ 3 it holds 1.2 to 1.7 times B's memory on curved bands and 2.4 times on a
plate, and its erosion is far smaller than the exact one. A caller wanting exact voxel
reach can use value morphology, a min or max over k steps, on leaves A has added; the
human decides between A and B.
