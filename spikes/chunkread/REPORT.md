# Spike: chunkread (B-027)

Measured on 2026-11-16 with rustc 1.98.1, release build, on an Apple M5 (macOS 27.2.1, its
internal SSD), against brodo at `3471e70`, 10 rayon threads; every case started at a
one-minute load of 2.22 to 3.30 and a five-minute load of 2.02 to 2.28. Reproduce from the
repository's root:

```bash
cargo build --release --manifest-path spikes/chunkread/Cargo.toml
spikes/chunkread/target/release/chunkread-spike 512 4 7 > out/spike/chunkread/run-a.tsv
```

`run-a.tsv` and `run-b.tsv` in `out/spike/chunkread/` hold the two runs.

## Question

**B-027**: how does an archive's reader take only the bytes of the chunks a region, a
listing or a grid by name needs, and what does it save against reading the file whole, as
`archive::read_region` and `archive::list` read it today (ADR-0016)?

## Method

The archive is four grids of `f32`, each a dense cube of 512³ voxels, every leaf present
(262 144 leaves, 537 MB of values), a smooth field differing by grid, written exact by
`archive::to_bytes`: 2 155 904 736 bytes, 513 chunks a grid.

The seeking reader is written from docs/formats.md §8 alone. It reads the header and the
directory (two calls), the grid's metadata and chunk table (one call, they are adjacent),
the first chunk, which it decompresses first, and then the leaf chunks the box needs, each
decompressed into a buffer of its own on rayon. Which leaf chunks a box needs is taken from
brodo's own region read, done once outside the time; the walk that finds them from the
first chunk's nodes took under 0.01 ms in `spikes/region/`. Every leaf the seeking reader
decompressed was compared with brodo's `RegionView::leaf`, outside the time, and was the same.

Variants:

- `whole`: the file read whole, then `archive::region_from_bytes` or `list_from_bytes`,
  as brodo reads it today;
- `seek_merged`: each run of neighbouring chunks read in one call;
- `seek_each`: one call a chunk;
- `seek_parallel`: one call a chunk, the calls on rayon;
- `seek_merged_all_tables`: `seek_merged` with every other entry's metadata and chunk
  table read too, which is what the structure checks of docs/formats.md §8.6 rule 1 need;
- `_warm`: the same, the file in the page cache.

Cold runs drop the file's pages from the page cache before each run (`mmap`, then `msync`
with `MS_INVALIDATE`), and read through a descriptor with `F_NOCACHE`. On this machine that
brought a 2 GiB file's read from 16 GB/s, the page cache's, to 6.5 GB/s, the disk's, run
after run (measured before the spike with a 2 GiB file of random bytes). Seven rounds,
the variants in turn, each on a fresh descriptor; fastest kept. The two runs agree within
7 % on every cold time but the 0.2 ms `seek_meta_tables` (25 %), and within 17 % warm.

## Measurements

`run-a`'s fastest, cold unless marked:

| Case | Leaf chunks | `whole` | `seek_merged` | `seek_each` | `seek_parallel` | `all_tables` | `whole` warm | `seek_merged` warm |
| :--- | ---: | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| one leaf | 1 | 337.6 ms | 0.72 ms | 0.59 ms | 0.59 ms | 0.75 ms | 410.7 ms | 0.12 ms |
| box of side 0.1 | 8 | 336.8 ms | 3.11 ms | 3.00 ms | 1.80 ms | 3.13 ms | 403.5 ms | 0.76 ms |
| box of side 0.5 | 64 | 336.8 ms | 13.5 ms | 23.4 ms | 10.4 ms | 13.6 ms | 398.6 ms | 6.14 ms |

Bytes read, the same in both runs:

| Case | `whole` | seeking | Calls, merged / each |
| :--- | ---: | ---: | :--- |
| one leaf | 2 155 904 736 | 1 058 097 | 5 / 5 |
| box of side 0.1 | 2 155 904 736 | 8 426 927 | 12 / 12 |
| box of side 0.5 | 2 155 904 736 | 67 378 347 | 12 / 68 |
| every other entry's metadata and table, added | | 12 888 | 3 |

A listing, and a grid found by name and read whole as the region of its bounds:

| Case | `whole` | Seeking | Bytes read, seeking |
| :--- | :--- | :--- | ---: |
| listing, metadata alone | 338.5 ms | 0.44 ms | 848 |
| listing, metadata and tables | | 0.22 ms | 17 344 |
| one of four grids by name, merged | 410.6 ms | 95.9 ms | 538 976 129 |
| one of four grids by name, in parallel | | 80.8 ms | 538 968 549 |

## Findings

1. A seeking reader's time follows the bytes it reads, plus about 0.2 ms a cold call: a
   leaf's region of a 2.16 GB archive in 0.6 ms against 337 ms, a listing in under 0.5 ms
   against 338 ms, one grid of four in 81 to 96 ms against 411 ms. A whole read also holds
   the whole file in memory (2.16 GB here) while a seeking one holds the chunks it read.
2. Reading every other entry's metadata and chunk table adds 12 888 bytes and 0.03 ms here,
   so a seeking reader can keep the structure checks of docs/formats.md §8.6 and §8.7
   rule 1 for every entry: each part's place, the chunk lengths summing to the payload, the
   file's length a multiple of 32 and its last part ending where the file ends. What it
   cannot keep without reading the file whole is §8.4 rule 3's padding of zeros between the
   parts it does not read, or that chunks it does not read exist beyond their stated
   lengths.
3. Many neighbouring chunks are read faster as one call (13.5 ms against 23.4 ms for 64
   chunks); calls in parallel are faster still (10.4 ms), and so for a few scattered ones
   (1.80 against 3.11 ms for 8). With the file in the page cache, every seeking read is
   at least 60 times faster than a whole read here.
4. A whole read was slower warm than cold (about 400 against 337 ms): read with `F_NOCACHE`
   the bytes go to the buffer without the page cache's copy. That is a property of the
   whole read, not of the seeking reader, and it is not explored further.
