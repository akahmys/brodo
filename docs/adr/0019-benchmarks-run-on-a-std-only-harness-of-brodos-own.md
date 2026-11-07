# ADR-0019: Benchmarks run on a std-only harness of brodo's own, and fail 10 % behind their baseline

- **Status**: Amended
- **Date**: 2026-10-17

Condensed on 2026-11-08 (ADR-0001): each decision is kept with its reason and what it set
aside; the runs that led to them are in git, this file as of `53f69d5`.

## Context

TESTING.md T5 asks a performance comparison for the fastest of many runs of each variant,
the variants run in turn, each run from evicted caches, each comparison in a fresh
process, and the whole run twice with its agreement stated. B-007 asked which harness
does this. The human limited the spike `spikes/bench/` to a harness of brodo's own,
measured alone; criterion and divan were not tried. On an Apple M5 on 2026-10-17
(`spikes/bench/REPORT.md`), about 150 lines of std met every clause of T5. At 20 rounds
with a 256 MiB eviction buffer, the two fresh processes of a pair agreed within 1.7 %;
5 rounds disagreed by up to 4.6 %, and 32 MiB left most of the working set cached.
Absolute times drifted by up to 7.9 % within one session of about 40 minutes.

## Decision

- **The harness.** The benchmarks are `harness = false` targets on std only, with no
  dependency added; the runner that compares them with their baselines is part of them.
  It lives in `crates/brodo/benches/harness/mod.rs`, colino's benchmark including it by
  `#[path]` rather than a second harness (2026-10-20); each crate writes to
  `out/bench/<crate>/`.
- **A comparison.** Pairs of fresh processes; each runs every variant once per round, in
  turn, for 20 rounds, writing over a 256 MiB buffer before each run, and keeps each
  variant's fastest time.
- **The tolerance.** An item fails its baseline when both runs of a pair are more than
  10 % slower. The human chose 10 % above the drift measured in one session (7.9 %).
- **Short items repeated** (2026-10-17). An item under 3 ms is repeated until a run lasts
  3 ms, the count in its name (`cache_map_x256`), so that a baseline taken at another
  count is not compared: under 3 ms, a pair disagreed by up to 14 %.
- **At rest** (2026-10-17, 2026-10-26). Benchmarks run on a machine doing nothing else. A
  run, comparison or record, waits while the one-minute load average is above 3.0
  (`BUSY_LOAD`) or the five-minute above 4.0 (`SETTLED_LOAD`), up to ten minutes, then
  refuses. A baseline recorded while other work ran had failed an unchanged item by 21 to
  27 %, its two children slow alike, so the spread within a pair could not see it; 0.5 was
  tried and refused a run after one wait. The five-minute load was added after runs that
  started below 3.0 just after a build, the system still scanning new binaries, disagreed
  by 16.9 %. The report's header names both loads, the hardware and the date in UTC
  (2026-10-25).
- **A pair that disagrees is refused** (2026-10-30). The harness refuses a pair whose two
  children disagree on an item by more than 10 % (`SPREAD`), comparing or recording, and
  judges nothing from it: a load read at the start cannot see work that begins during a
  run, and at rest pairs agreed within 7.8 %. A refused pair is run again.
- **Recording one item** (2026-10-20). `--record NAME` replaces or adds that item's row
  alone, with the faster of the pair, the other rows kept with their dates; an unknown name
  is refused. `--record` alone records every item. Recording every row to add one would
  reset the rows drift is measured against.
- **Every feature** (2026-10-30). Every benchmark runs with every feature, as the gate's
  tests do (`cargo bench -p <crate> --bench <crate> --all-features`, TESTING.md T10), so
  that an item behind a non-default feature, the archive's, is not left out unseen.
- **Threads** (2026-10-31). A benchmark's children run on as many rayon threads as the
  machine has performance cores (`hw.perflevel0.logicalcpu` on macOS; every processor where
  the system names no core types), the pool built before an item's data. On ten threads of
  an M5's four performance and six efficiency cores, the leaf-parallel update varied with
  the time its threads waited for a processor (correlation +0.96) and `convert_slab_320`
  by up to 32 % with instructions per cycle lost on both core types, no cause the counters
  showed; on four threads both agreed within 1.3 % (`spikes/parvar/`, B-024). A spike, not
  a benchmark, compares thread counts.
- **When benchmarks run** (2026-10-17, 2026-11-04). A change to `src/` on a path a benchmark
  times is benchmarked before it lands (TESTING.md T10), after a refactoring made gather
  50 % slower unseen. `commit_msg_check.py` holds part of it: a commit changing a crate's
  `src/` has a line beginning `Benchmarks`, and in the hook, unless it says
  `Benchmarks: not on a timed path`, the crate's last report is newer than the staged
  files and every item `ok`. Version 2's reads had landed without a benchmark and the
  cache map was found 28 times slower at the milestone's close.
- **A baseline moved** only by the human, an item at a time with `--record NAME`, when a
  change's cost is accepted or a drift is judged. Each such run and its figures are
  stated in the commit and ROADMAP.md (ADR-0001).

## Consequences

- A slowdown under 10 % passes unseen by a baseline comparison; comparing variants in turn
  in one process remains the way to see one.
- Code placement alone moves an item by about 15 %: at P5's close `gather` went from 3.09
  to 3.58 ms between two builds whose code on its path disassembled the same, only its
  addresses moved. The tolerance cannot tell this from a change in the code; disassembling can.
- `leaf_parallel_update_x32` drifts between processes on four threads by up to 20 %, its
  cause not measured: it was found 10 to 21 % slow at P7's and P9's closes with the code
  on its path unchanged, recorded again at the first and left at the second (2026-11-08).
- The eviction size, the loads and the thread count were measured on one Apple M5 alone.
- colino's benchmark, including a file outside its crate, cannot be packaged; the packages
  leave benchmarks out (TESTING.md V8).
