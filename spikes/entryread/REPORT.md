# Spike: entryread (B-032)

Plan and pass conditions written on 2026-11-27, before any case was timed (PLANNING.md §3,
§5). Reproduce from the repository's root:

```bash
spikes/entryread/run.sh 20
```

## Question

**B-032** ([Issue 24](https://github.com/akahmys/brodo/issues/24)): `archive::read` at a path
holds the whole compressed file and the cache file it makes at once. Read from the file's
start to its end, one entry at a time, each entry's chunks taken and decompressed before
the next entry is read, it would hold the cache file and one entry's compressed bytes. How
much memory does that save, and how much time does it give up, since chunks of different
entries are then no longer decompressed together?

## Method

Two readers, written from docs/formats.md §3 and §8 alone, for archives of exact values:

- `whole`: the file read whole by `std::fs::read`, then every entry's chunks decompressed
  together on rayon, as `archive::read` does today;
- `entry`: the file read from its start to its end without a seek, so that a pipe reads
  as today: the header and directory, then each entry's metadata and chunk table, then its
  chunks, decompressed on rayon before the next entry is read.

Each gives the cache file's bytes; before anything is timed, both are held to
`cache::to_bytes` of the same entries, and a case on which either differs stops the run.
Neither checks the cache file as §3.4 asks, which is the same work for both; `brodo`,
`archive::read` itself, is timed beside them for the cost of a whole read with its checks.

Cases, grids of `f32` with every leaf of a box present, a smooth field differing by grid,
written exact by `archive::write`:

| Case | Grids | Leaves a grid | Leaf chunks a grid |
| :--- | ---: | ---: | ---: |
| `one_large` | 1 | 32 768 | 64 |
| `four_large` | 4 | 32 768 | 64 |
| `one_medium` | 1 | 2 048 | 4 |
| `many_medium` | 64 | 1 024 | 2 |
| `many_small` | 256 | 64 | 1 |

Time: on as many rayon threads as the machine has performance cores, each case's variants
in turn, starting from a different one each round, 256 MiB written over before each run,
the file in the page cache as brodo's benchmark reads it; the fastest of 20 rounds kept, in
two processes. Memory: each case and variant read once in a process of its own, its peak
resident memory by `/usr/bin/time -l`, beside `none`, the process reading nothing.

## Pass conditions

Written before measuring:

1. **Bytes**: `whole` and `entry` give the cache file of the same entries on every case.
2. **Runs**: the two processes agree within 10 % on every time; otherwise the pair is
   refused and run again, at rest (TESTING.md T5).
3. **Time**: on every case, in both runs, `entry` is slower than `whole` by at most 10 % of
   `brodo`'s time on that case. A case beyond it is recorded, and the owner decides.
4. **Memory**: on `four_large`, `entry`'s peak is below `whole`'s by at least half of the
   archive's length less its largest entry's; on the cases of one grid, `entry`'s peak is
   at most 5 % above `whole`'s.

## Measurements

2026-11-27, Apple M5, 4 threads, load 1.8 to 2.2 throughout; `out/spike/entryread/`.

**Runs refused.** The first pair failed condition 2 (`brodo` on `four_large`, 39.8 and
31.1 ms). It also showed `entry` 1.6 ms slower than `whole` on `one_large`, a single grid,
where the two do the same work: `entry` zeroed its 64 MiB of chunks before reading them,
which `std::fs::read` does not. That was the spike's defect, not the method's; `entry` now
reads through `take(..).read_to_end`, as `std::fs::read` does. The second pair failed
condition 2 on `one_medium` (`brodo`, 0.50 and 0.60 ms). The third pair, below, passed it:
every time within 9 % between the processes.

**Bytes** (condition 1): both readers give `cache::to_bytes` on every case. Passed.

Fastest of 20 rounds, ms, run a / run b:

| Case | `brodo` | `whole` | `entry` | `entry` − `whole`, of `brodo` |
| :--- | ---: | ---: | ---: | ---: |
| `one_large` | 7.40 / 7.36 | 5.30 / 5.29 | 5.29 / 5.23 | 0 % / −1 % |
| `four_large` | 28.06 / 27.78 | 20.84 / 20.79 | 21.04 / 20.95 | +1 % / +1 % |
| `one_medium` | 0.49 / 0.48 | 0.41 / 0.42 | 0.40 / 0.36 | −2 % / −11 % |
| `many_medium` | 14.72 / 14.49 | 11.53 / 11.49 | 10.73 / 10.64 | −5 % / −6 % |
| `many_small` | 4.96 / 5.07 | 3.07 / 3.03 | 6.65 / 6.80 | **+72 % / +74 %** |

**Time** (condition 3): passed on four cases; **failed on `many_small`**, 256 grids of one
chunk each, where `entry` takes twice `whole`'s time: each entry's single chunk is
decompressed alone, so rayon has one task at a time to run on four threads.

Peak resident memory, MiB (`none`, the process reading nothing: 2.2):

| Case | Archive | `brodo` | `whole` | `entry` | `whole` − `entry` | Required |
| :--- | ---: | ---: | ---: | ---: | ---: | ---: |
| `one_large` | 64.2 | 132.8 | 130.5 | 130.5 | 0 | ≥ −6.5 (5 %) |
| `four_large` | 257.0 | 517.7 | 515.4 | 386.9 | 128.5 | ≥ 96.4 |
| `one_medium` | 4.0 | 12.5 | 10.3 | 10.3 | 0 | ≥ −0.5 (5 %) |
| `many_medium` | 128.5 | 261.8 | 259.4 | 132.9 | 126.5 | — |
| `many_small` | 32.2 | 71.4 | 68.9 | 36.9 | 32.1 | — |

**Memory** (condition 4): passed. Reading entry by entry holds the cache file and one
entry: on many grids the peak halves, and on one grid it is unchanged.

## Result

Conditions 1, 2 and 4 pass; condition 3 fails on `many_small` alone. Entry by entry, the
memory the issue asks for is saved on every case of more than one grid, at no cost in time
but where entries are of one chunk each, and there at the cost of 3.6 ms in 5, twice the
decompression's time. Which to take, and whether a reader that gathers small entries'
chunks together before decompressing them is worth its added rule, is the owner's
decision (condition 3).

## Decision

The owner took entry by entry on 2026-11-28, the slower case on `many_small` accepted
(ADR-0016's amendment of 2026-11-28).
