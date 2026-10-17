# ADR-0023: Mutation testing closes a milestone, behind a memory guard

- **Status**: Accepted
- **Date**: 2026-10-18

## Context

The audit review before P1 closed (proposal C) asked whether brodo's tests catch the
defects they are meant to catch, beyond T3's one break per new test. `spikes/mutants/`
measured cargo-mutants 27.1.0 (MIT; its lock file pulls no C) on three of brodo's modules,
228 mutants: every mutant that differed from the original in behaviour was caught; the
three missed were equivalent. A caught mutant cost 13 to 16 s and the crate's 1099 about
5 hours at one job. A first run at four jobs took the machine down: mutants in
`Mask::ones` made an iterator endless, three test processes reached 47 GiB together, and
the kernel panicked. macOS enforces neither `ulimit -v`, `-d` nor `-m`.

## Decision

- TESTING.md T11: before a milestone closes, every mutant of the workspace is run, by
  `scripts/audit/mutants.py --run`. It sets `scripts/audit/rss_guard.py` as cargo's runner
  for the host, which kills a test binary past 1 GiB (thirty times the largest peak of the
  unmodified tests, 32 MiB), keeps cargo-mutants' copies under `target/`, and runs two
  jobs. It fails on a missed mutant, and on a timeout before any test failed.
- A missed mutant is killed by a new test, or excluded as equivalent in
  `.cargo/mutants.toml`'s `exclude_re`, a regex without line numbers under a comment
  saying why. Like `#[expect]`, an exclusion needs no ADR; its reason is reviewed with
  the diff. The gate (`mutants.py` with no argument) fails on an exclusion without a
  comment, and on one that no longer excludes a mutant, by cargo-mutants' own regex
  engine, so cargo-mutants joins the audit's tools.
- Not the attribute `#[mutants::skip]`, which needs the `mutants` crate as a regular
  dependency.
- The timeout is twice the unmutated suite's time (`timeout_multiplier`). In the spike,
  each of the 7 mutants still running then had already failed 18 to 29 tests; libtest runs
  a binary's remaining tests after a failure, and a labelling test on a broken topology
  outran the 431 s timeout each time. Stopping a binary at its first failure needs a nightly harness flag.

Shown to fire: the gate's check failed on a pattern with its comment removed, on a
pattern matching no mutant, and on a double-quoted pattern; `--report` failed on the
spike's report, with its 2 missed mutants, and on a timeout whose log had its failures
removed, and passed with neither; `--run` on `manifest.rs` excluded its equivalent mutant,
ran the other 42 in 7 minutes under a 168 s timeout with every test binary logged by the
guard, and passed, and failed once the runner was not set; `rss_guard.py` killed a
process allocating 4 GiB under a 1 GiB limit, and during the spike 5 mutants at 1025 to
1121 MiB.

## Consequences

Closing a milestone takes hours more, run at rest. A mutant that only exhausts memory
counts as caught, since the guard's kill fails the test binary. Doctests run through
rustdoc, outside the guard; brodo has none. Nothing checks that the run happened; the
milestone's checklist lists it.

## Amendment

- **Date**: 2026-10-18

A review found two gaps, and the human decided both.

- `.cargo/mutants.toml` held the run's settings (`timeout_multiplier`, `output`) beside
  the exclusions, so a setting of a rule could change with no ADR, since the file is not
  a rule file. The settings moved into `mutants.py`, which passes them on the command
  line; the file holds the exclusions only, and the gate fails on any other key or table.
  Shown to fire: a `timeout_multiplier` line and a `[profile]` table each failed it.
- `rss_guard.py` measured and killed the test binary alone, so a process it started
  could grow unwatched and outlive it. The guard now sums and kills the binary with
  every process descended from it, followed by parent pid, and kills those left when the
  binary ends. It does not give the binary a session of its own: cargo-mutants puts cargo
  in a process group and ends a timed-out test by `killpg(SIGTERM)` on that group
  (`process/unix.rs` in 27.1.0), which a new session would escape. Shown to fire: a
  grandchild allocating 3 GiB was killed with its parent at 1292 MiB, where the previous
  guard let it reach 3084048 KiB; a `sleep` left by a parent that exited was killed, where
  the previous guard left it running. No test of brodo starts a process: `cargo test` runs
  13 binaries, none of them the benchmark, which does. `--run` on `manifest.rs` passed
  again: 42 mutants in 7 minutes, the timeout 162 s from the command line's multiplier,
  every binary logged by the guard.
