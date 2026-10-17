# ADR-0019: Benchmarks run on a std-only harness of brodo's own, and fail 10 % behind their baseline

- **Status**: Accepted
- **Date**: 2026-10-17

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

- The benchmarks are `harness = false` targets in `crates/brodo/benches/`, on std only,
  with no dependency added. The runner that compares them with their baselines is part of
  the same benches, not a separate tool.
- Each comparison runs in pairs of fresh processes; each process runs every variant once
  per round, in turn, for 20 rounds, writing over a 256 MiB buffer before each run, and
  keeps each variant's fastest time.
- A benchmark fails its baseline when it is more than 10 % slower. The human chose 10 %
  above the drift measured in one session (7.9 %).

## Consequences

The drift between days, between the performance and efficiency cores, and with the
machine's temperature was not measured, so 10 % may fail a change that made nothing
slower; such a failure is a reason to measure the drift again and amend this record. A
slowdown under 10 % passes unseen
by a baseline comparison; comparing variants in turn in one process remains the way to
see one. The eviction size was measured on one machine only.
