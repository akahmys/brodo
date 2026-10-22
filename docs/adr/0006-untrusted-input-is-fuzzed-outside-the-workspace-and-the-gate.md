# ADR-0006: Untrusted input is fuzzed, outside the workspace and the gate

- **Status**: Amended
- **Date**: 2026-10-16

## Context

A crate that reads input the project did not write meets only the hostile inputs its
tests think of. A fuzzer finds the reads past the end, overflows, endless loops and
unbounded allocations that tests do not. `cargo-fuzz` builds on nightly and links
libFuzzer, which is C++, while the gate runs on a pinned stable toolchain and Rule 12
keeps C and C++ out of what the project builds.

## Decision

- Every crate listed in `UNTRUSTED_INPUT` has a fuzz target for each way in which it
  reads untrusted input. A refusal is an answer; a panic, a hang past 60 s or an
  allocation past 2 GiB is a finding.
- `fuzz/` is a crate of its own, outside the workspace and with its own `[workspace]`
  table, as `spikes/` is. Nothing in `crates/` depends on it, so libFuzzer never reaches
  what the project builds, Rule 12 and `deny.toml` have nothing new to judge, and the gate
  keeps its toolchain.
- Before a milestone closes, every target runs 900 s with no finding (TESTING.md T6); a
  finding becomes a test that fails without its fix. There is no hosted CI, so nothing
  runs it nightly.
- The audit checks that the fuzzer exists and reaches each such crate; it does not run it.

## Consequences

Running the fuzzer needs nightly and `cargo install cargo-fuzz`; the gate does not. A
project that grows a runner (an `xtask fuzz` that seeds corpora and counts findings)
records it by amending this record. The corpus and findings are not committed.

## Amendment

- **Date**: 2026-10-16

`UNTRUSTED_INPUT` lists parsing modules rather than crates (ADR-0009); the crate of each
listed module is fuzzed. Miri, which also builds on nightly, runs outside the gate in the
same way (ADR-0008).

## Amendment

- **Date**: 2026-10-22

P3 added a target for Hermite data in a cache file, which random bytes almost never reach:
a valid cache file with ten keyed grids is far from any short random input. The human
decided that a target whose input has such a structure starts from the committed files
under `crates/brodo/tests/data/` as seeds (TESTING.md §3). Held by nothing; the
milestone's checklist lists the fuzz round.

