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

## Answer proposed for the ADR

Replace rayon in `brodo::parallel` with a dynamic split on `std::thread::scope`, blocks of
16 leaves; `rayon` and the `crossbeam` crates leave the dependencies, and Miri can check
the parallel code. Left to the human: whether the 4 to 6 % on small grids is acceptable,
and how the thread count is chosen, since rayon's pool and its `install` (used by the
tests to vary the thread count) go with it.
