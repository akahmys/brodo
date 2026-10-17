# Spike: mutants (the audit review's proposal C)

Measured on 2026-10-18 with cargo-mutants 27.1.0 (installed with `cargo install --locked`,
as the human allowed) and rustc 1.98.1, debug builds, on an Apple M5 (10 cores, 32 GB).
Reproduced from the repository's root, now through the script T11 settled on (TESTING.md
§3), which adds the guard below; the spike ran the same command by hand, with
`--no-shuffle` and the jobs stated:

```bash
python3 scripts/audit/mutants.py --run -f crates/brodo/src/tree.rs
```

## Question

Do brodo's tests catch the defects a mutation tool puts into the code, what does a run
cost, and can it run safely on this machine?

## The tool

cargo-mutants is MIT (its crates.io record and `Cargo.toml`). Its `Cargo.lock` at tag
v27.1.0 lists 204 packages, none of them `cc`, `cmake`, `pkg-config` or `bindgen`, so it
compiles no C (Rule 12 asks this of dependencies only; the tool is not one). It copies the
tree, excluding `target/`, and for each mutant runs `cargo test --no-run` (failure:
"unviable"), then `cargo test` (a failure: "caught"; a pass: "missed"; too slow:
"timeout"). Skipping a function by attribute needs the `mutants` crate as a regular
dependency (mutants.rs, "attrs"); a regex in a config file (`exclude_re`, or `--config`)
needs none.

## Measurements

`cargo mutants -p brodo --list` gives 1099 mutants: `analysis.rs` 230, `tree.rs` 157,
`topology.rs` 126, `cache.rs` 102, the rest fewer than 91 each. The unmutated baseline
takes 6 to 7 s to build and 79 to 86 s to test.

| Files | Jobs | Mutants | Wall time | Caught | Timeout | Unviable | Missed |
| :--- | :-: | -: | -: | -: | -: | -: | -: |
| `gather.rs`, `manifest.rs` | 1 | 71 | 19 min 34 s | 62 | 0 | 8 | 1 |
| `tree.rs`, until the crash | 4 | 74 | about 15 min | 73 | 0 | 1 | 0 |
| `tree.rs`, the rest (`--iterate`) | 2 | 83 | 33 min 49 s | 61 | 7 | 13 | 2 |

The crashed run had also finished 9 mutants (2 missed, 7 timeouts); `--iterate` kept only
the caught and unviable ones and ran those 9 again, with the same outcomes.

**Every missed mutant is equivalent**, checked by hand:

- `manifest.rs:45` `<` → `<=` in `Offset::opposite`: `cell()` skips 13, so `26 - cell()` is
  never 13.
- `tree.rs:39` `|` → `^`, twice, in `child_place`: x, y and z are each masked to
  `dim_log2` bits and shifted apart, so no two of them share a bit.

**Every timeout is caught**: 18 to 29 tests had already failed when the run timed out
(431 s, five times the baseline). libtest runs the rest of a binary after a failure, and
`analysis::tests::background_enclosed_by_leaves_is_apart_from_the_outside` then outran
the timeout every time; by hand, under `child_index → 0`, it was still running after 3
minutes. Sampled under `child_index → 0` it spent its time in `Walk::run`, `pair` and
`classify` at a constant 8.6 MiB: the walk is bounded by the tree's four levels, but with
the topology broken every region classifies as split and it visits all of a node. That is
slow, not endless, so Rule 16 is not at issue. The 7 timeouts took 25 of the 34 minutes.

A mutant that is caught costs 13 to 16 s; one that is missed, the whole suite (about 80 s).
At one job, 16.5 s a mutant gives about 5 hours for all 1099; two jobs about halve it.

## The crash

The first `tree.rs` run, at four jobs, brought the machine down: a kernel panic, "watchdog
timeout: no checkins from watchdogd in 94 seconds", with the memory compressor at 100 % of
its segments and 87 swap files. Three mutated `brodo` test binaries held 23.4, 14.2 and
10.1 GiB, beside a 13.4 GiB virtual machine. The mutants under test were in `Mask::ones`
(`bits &= bits - 1` with `+`, `/` or `|=`): `bits` never reaches zero, the iterator never
ends, and what collects it grows without bound.

macOS refuses `ulimit -v`, `-d` and `-m` ("cannot modify limit: Invalid argument"), so
`rss_guard.py` (now `scripts/audit/rss_guard.py`) runs each test binary as cargo's runner and kills it once its resident
memory passes `RSS_GUARD_MIB`. Shown to fire (TESTING.md T3): a process allocating 4 GiB
under a 1024 MiB limit was killed at 1921 MiB, and passed under 8192 MiB. Under it, the
unmutated suite's 13 test binaries each logged one line, the largest peak 32 MiB
(`spheres`); doctests run through rustdoc and are not guarded. In the resumed run it
killed 5 mutants at 1025 to 1121 MiB, and the machine stayed up.

## Answer

brodo's tests caught every non-equivalent mutant in three modules (228 mutants, 3
equivalent). A whole-crate run takes hours, so it suits a milestone's close, as fuzzing
and Miri do (TESTING.md T6, T7), not the gate. It is safe only behind a memory guard, and
`--timeout` near twice the baseline would cut most of the timeouts' cost. Not measured:
`analysis.rs`, `topology.rs`, `cache.rs` and the rest of the crate, and `colino`.
