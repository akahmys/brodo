# Spike: threads (B-014)

Measured on 2026-10-17 with rustc 1.98.1 (release) and Miri 0.1.0 (nightly of
2026-07-21), on an Apple M5 (10 cores), one-minute load average 2.38 to 2.48. Reproduce
from this directory:

```bash
cargo build --release
./target/release/threads-spike                  # bits: each variant against rayon
./target/release/threads-spike bench 20 3       # speed, T5; RADIUS=160 for the larger grid
THREADS=4 MIRIFLAGS=-Zmiri-disable-isolation cargo +nightly miri run -- miri dynamic
```

## Question

**B-014**: can `std::thread::scope` replace rayon for brodo's leaf-parallel update, so that
`rayon`, `rayon-core` and the `crossbeam` crates leave the dependencies? Miri's Stacked
Borrows reported undefined behaviour inside `crossbeam-epoch` 0.9.21 under rayon
(ADR-0008).

## Candidates

The same work, one diffusion step per leaf (gather, then the 7-point Laplacian), written
into a `Vec<[f32; 512]>` by leaf number:

| | |
| :--- | :--- |
| rayon | `par_iter_mut` over the leaves, as `brodo::parallel::par_update` does |
| static | `std::thread::scope`, one contiguous run of leaves per thread |
| dynamic | `std::thread::scope`, blocks of 16 leaves taken by whichever thread is free, through a `Mutex` over the blocks |

"Skewed" repeats the step 8 times on leaves with x ≥ 40, so that the work per leaf is
uneven.

## Measurements

Fastest of 20 rounds, the variants in turn, a 256 MiB eviction before each run, pairs of
fresh processes (TESTING.md T5); ranges over the children.

| Grid | Variant | Uniform | Skewed |
| :--- | :--- | ---: | ---: |
| sphere r = 80, 2606 leaves | rayon | 0.677–0.702 ms | 0.930–0.974 ms |
| | static | 0.859–0.872 ms | 1.146–1.193 ms |
| | dynamic | 0.727–0.744 ms | 0.974–1.017 ms |
| sphere r = 160, 10 469 leaves | rayon | 2.690–2.771 ms | 4.279–4.340 ms |
| | static | 3.169–3.206 ms | 5.039–5.263 ms |
| | dynamic | 2.661–2.707 ms | 4.184–4.248 ms |

Pairs agreed within 4.4 %. Every variant's output had the same bits as rayon's, on both
grids and both workloads.

Under Miri, with four threads set explicitly (Miri reports one core, so without it every
variant ran on one thread and tested no concurrency), on a sphere of radius 4 (17
leaves): static and dynamic ran to the end with no report; rayon reproduced the Stacked
Borrows report in `crossbeam-epoch` 0.9.21. Miri ran one interleaving of each.

### Smaller grids, and callers already on rayon

The fixed cost of spawning per call, uniform work: on 120 leaves rayon took 0.086–0.091 ms
and the dynamic split 0.120–0.145 ms; on 434 leaves, 0.160–0.166 and 0.203–0.218 ms; about
40 to 50 µs a call throughout.

`nested <rounds>` runs ten updates under an outer rayon `par_iter`, as a caller already on
rayon would, fastest of 20, two runs each:

| Grid | inner rayon | inner dynamic, 10 threads each | inner on one thread | outer sequential, inner rayon |
| :--- | ---: | ---: | ---: | ---: |
| 2606 leaves | 6.18–6.22 ms | 6.67–6.68 ms | 6.93–6.94 ms | 6.52–6.55 ms |
| 120 leaves | 0.297–0.298 ms | 0.81–0.90 ms | 0.39–0.41 ms | 0.46 ms |

Nested rayon was fastest in both: its pool shares the work of inner and outer loops.
Scoped threads inside it oversubscribe the cores (about a hundred threads), 8 % slower on
the larger grid and about three times on the smaller; one inner thread each avoids that
but leaves the ten outer tasks unbalanced across performance and efficiency cores, 12 to
30 % slower than nested rayon.

### Miri's two models

Rayon with a pool of four threads (checked by `rayon::current_num_threads`) on the
17-leaf grid: Stacked Borrows reported undefined behaviour in `crossbeam-epoch`, as above;
Tree Borrows ran to the end with no report (`-Zmiri-tree-borrows`, about two minutes).
The report depends on the model.

## Findings

1. A dynamic split on scoped threads matched rayon on the larger grid, within 1 to 3 %
   faster, and was 4 to 6 % slower on the smaller one, where a run lasts 0.7 ms and
   spawning ten threads per call weighs; rayon keeps its threads.
2. A static split was 18 to 25 % slower than rayon on both, even with uniform work: threads
   finish at different times however the leaves are divided.
3. The dynamic split ran under Miri with concurrency and no report; rayon's report came
   from `crossbeam-epoch`, which the dynamic split does not use.
4. Determinism holds: the update writes each leaf from its own inputs, so the split does
   not change the bits.
5. A caller already on rayon is served best by rayon: scoped threads either oversubscribe
   (8 % to three times slower here) or, with one inner thread, lose the balancing rayon
   does across the two kinds of core (12 to 30 %).
6. Rayon's Miri report appears under Stacked Borrows and not under Tree Borrows.

## Answer proposed for the ADR

Open again: the measurements of finding 5 and 6 weigh for keeping rayon, and running
Miri on the parallel code under Tree Borrows; the human decides.
