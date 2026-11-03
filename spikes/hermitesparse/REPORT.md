# Spike: Hermite data held sparsely (B-023)

Measured on 2026-11-03 with rustc 1.98.1 on an Apple M5 of 32 GB, against brodo at
`75eae29`. Peak resident memory does not depend on load; the times do, and were taken at a
load average of about 11, another program using eight cores, so they compare the two
layouts in one run and are not baselines. Reproduce from the repository's root:

```bash
spikes/hermitesparse/run.sh out/hermitesparse
```

## Question

ROADMAP.md §3, B-023: does Hermite data hold its crossings only on the edges that hold
one, rather than nine dense fields over every leaf voxel? P5's review found the dense set
about 61 bytes a voxel, so that a sphere of radius 1500 voxels would not fit in 32 GB.

## Method

`src/main.rs` makes Hermite data of an analytic sphere, its centre off the lattice: a
point's sign by whether it lies within, an edge's crossing where the sphere meets it, its
normal the radius there. The topology is a narrow band's of 3 voxels, as colino's
conversion makes, with inside tiles at the largest level that fits. Then, by layout:

- **dense**: brodo's set at format version 1, ten fields, made by `Hermite::new`, which
  checks it whole, and written by `Hermite::entries` to a cache file;
- **sparse**: brodo's sign field, and per leaf and axis a mask of the edges holding a
  crossing (64 bytes), where each leaf's crossings start (4 bytes), and the crossings in
  leaf, axis and place order, 20 bytes each (fraction, normal, group); written as the sign
  grid's cache file followed by the three arrays;
- **none**: the topology alone, the part both share.

## Results

| Layout | Radius | Leaves | Tiles | Crossings | Fill (s) | Write (s) | File (bytes) | Peak resident (bytes) |
| :--- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| none | 200 | 16 298 | 57 595 | 0 | | | | 11 124 736 |
| dense | 200 | 16 298 | 57 595 | 754 016 | 0.7 | 0.2 | 513 788 352 | 1 047 019 520 |
| sparse | 200 | 16 298 | 57 595 | 754 016 | 0.1 | 0.0 | 26 802 444 | 60 063 744 |
| none | 600 | 146 124 | 646 922 | 0 | | | | 111 542 272 |
| dense | 600 | 146 124 | 646 922 | 6 785 906 | 6.8 | 2.1 | 4 608 517 952 | 8 270 249 984 |
| sparse | 600 | 146 124 | 646 922 | 6 785 906 | 0.9 | 0.2 | 240 351 900 | 562 429 952 |
| none | 1500 | 911 037 | 4 840 811 | 0 | | | | 701 693 952 |
| sparse | 1500 | 911 037 | 4 840 811 | 42 411 518 | 5.8 | 1.1 | 1 500 996 656 | 3 593 191 424 |

- The crossings are about 1.5 × 4πr², a sphere's area times the mean of |nₓ| + |n_y| +
  |n_z|, as expected: 754 016 against 753 982 at radius 200.
- Dense was not run at radius 1500: at radius 600 it peaked at 56.6 KB a leaf, which at
  911 037 leaves is about 51.6 GB, beyond the machine's 32 GB. Its file would be about
  28.7 GB, 31.5 KB a leaf.
- Sparse took 15 to 17 times less memory and about 19 times fewer bytes, and filled and
  wrote 7 to 10 times faster. Its file at radius 1500, 1.50 GB, is the sign field's 512
  bytes a leaf (466 MB), the masks' 192 (175 MB), and the crossings' 20 each (848 MB).
- The sign field is a third of the sparse file. Held as bits, 64 bytes a leaf, it would
  save another 408 MB at radius 1500, about 27 %.

## Conclusion

Held sparsely, Hermite data of a sphere of radius 1500 voxels fits in 3.6 GB where the
dense set does not fit in 32 GB. The layout, where it lives in a file, and the version are
the owner's, in the ADR on B-023.
