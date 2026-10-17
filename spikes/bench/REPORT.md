# Spike: bench (B-007)

Measured on 2026-10-17 with rustc 1.98.1, release build, on an Apple M5 (Mac17,2; 10
cores; L2 16 MiB on the performance cores, 6 MiB on the efficiency cores, as `sysctl`
reports; no last-level cache size is reported). Reproduce from the repository's root:

```bash
cargo build --release --manifest-path spikes/bench/Cargo.toml
spikes/bench/target/release/bench-spike 256 20 5
```

The arguments are the eviction buffer in MiB, the rounds per child, and the pairs of
children.

## Question

**B-007**: which harness runs the benchmarks? The human limited the spike to a harness of
brodo's own, std only, measured alone, with the comparison runner inside the benches
crate; criterion and divan were not tried.

## The harness

Built to TESTING.md T5 (`src/main.rs`, about 150 lines with the workload):

- The parent runs pairs of child processes, each a fresh process (T5: each comparison in
  a fresh process; the whole run twice).
- A child runs every variant once per round, in turn, starting from a different variant
  each round; before each run it writes over an eviction buffer a cache line at a time.
- A child reports each variant's fastest and median time; the parent reports how far the
  two children of a pair agree on the fastest.

Workload: a narrow-band sphere SDF of radius 80 (2606 leaves, 3025 tiles), read at about
100 000 voxels by three variants: random voxels through a `ValueAccessor`, the same voxels
through `Field::value_at`, and a box of 46³ swept x fastest through an accessor.

## Measurements

Fastest time of `accessor_random`, 5 pairs (10 children) per row; "pair" is the largest
disagreement within a pair, "all" the range over the 10 children.

| Eviction | Rounds | Fastest | Pair | All | Wall time |
| ---: | ---: | :--- | ---: | ---: | ---: |
| none | 5 | 4.840–5.100 ms | 4.6 % | 5.4 % | 0.6 s |
| none | 20 | 4.776–4.830 ms | 0.8 % | 1.1 % | 2.2 s |
| none | 100 | 4.747–4.838 ms | 1.2 % | 1.9 % | 10.5 s |
| 32 MiB | 20 | 4.875–4.995 ms | 1.7 % | 2.5 % | 2.7 s |
| 256 MiB | 5 | 5.465–5.746 ms | 4.6 % | 5.1 % | 1.6 s |
| 256 MiB | 20 | 5.430–5.507 ms | 1.2 % | 1.4 % | 5.6 s |
| 256 MiB | 100 | 5.440–5.586 ms | 1.1 % | 2.7 % | 27.2 s |

`value_at_random` behaved alike (pair at most 1.4 % from 20 rounds on). `accessor_sweep`
took 0.183–0.185 ms in every row, eviction or not, every pair within 0.5 %.

**Eviction size.** Run in separate batches, 128, 512 and 1024 MiB gave 5.34–5.71,
5.62–5.76 and 5.57–5.71 ms. Run interleaved (256, 512, 1024 MiB, three times over, one
pair each), all three gave 5.34–5.59 ms with no order between them; the batch differences
were drift over time, not the buffer.

**Drift.** Over the session (about 40 minutes), `accessor_random` at 20 rounds with
256 MiB or more ranged 5.34–5.76 ms, 7.9 %, while each pair agreed within 1.7 %. The ratio
of `accessor_random` to `value_at_random` within one child, over 18 children, ranged
0.931–0.966, 3.8 %: no steadier than the times themselves.

**Checked to have an effect.** Eviction of 256 MiB made random access 13–14 % slower than
none (4.78 against 5.43 ms at 20 rounds); 32 MiB only 2 %, so less than the caches hold.

## Findings

1. A std-only harness of about 150 lines meets every clause of T5: fastest of many runs,
   variants in turn, eviction before each run, a fresh process per comparison, the run
   twice with its agreement stated.
2. 20 rounds are enough: the two runs of a pair agree within 1.7 %; 5 rounds disagree by
   up to 4.6 %, and 100 are no better than 20 at five times the time.
3. Eviction needs a buffer of 256 MiB on this machine; 32 MiB leaves most of the working
   set cached. Above 256 MiB nothing changed when measured interleaved.
4. Absolute times drift by up to 8 % within a session while pairs agree within 2 %. A
   comparison with a stored baseline therefore cannot detect a change smaller than the
   drift, about 8 % here; a comparison of variants run in turn in one process resolves
   the 5–6 % between `accessor_random` and `value_at_random` at every pair.
5. Nothing pins a process to a core type on macOS; the child may run on performance or
   efficiency cores, and the drift may include this. Not measured.

## Answer proposed for the ADR

The benchmarks run on a harness of brodo's own, std only, in `crates/brodo/benches/` with
`harness = false`: the parameters above (20 rounds, a 256 MiB eviction buffer, pairs of
fresh processes), and the comparison runner in the same benches crate. Left to the human:
the tolerance at which a baseline comparison fails, at least the measured drift (8 %).
