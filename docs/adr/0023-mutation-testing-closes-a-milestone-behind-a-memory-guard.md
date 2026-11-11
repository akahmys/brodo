# ADR-0023: Mutation testing closes a milestone, behind a memory guard

- **Status**: Amended
- **Date**: 2026-10-18

Condensed on 2026-11-08 (ADR-0001): each decision is kept with its reason and what it set
aside; the runs that led to them are in git, this file as of `53f69d5`.

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

- **The run.** TESTING.md T11: before a milestone closes, every mutant of the workspace is
  run, by `scripts/audit/mutants.py --run` and never without it, with `--all-features` as
  the gate's tests build (2026-10-25: run with the default features, the code behind
  opt-in features was mutated but its tests not built, and 148 mutants were missed). The
  milestones from P5 on close with the checks the human chooses (ADR-0033).
- **The guard.** `mutants.py` sets `scripts/audit/rss_guard.py` as cargo's runner for the
  host, which kills a test binary past 1 GiB (thirty times the unmodified tests' largest
  peak, 32 MiB), keeps cargo-mutants' copies under `target/`, and runs two jobs. The guard
  sums the binary with every process descended from it and kills those left when it ends
  (2026-10-18); it gives the binary no session of its own, since cargo-mutants ends a
  timed-out test by `killpg` on cargo's process group, which a new session would escape.
  A mutant applied by hand, to show a test fails on it (T3), runs behind the guard too, as
  TESTING.md §3 states (2026-10-27): one run unguarded took the machine down.
- **The timeout** is twice the unmutated suite's time. A mutant timing out before any test
  failed is run again by `mutants.py --run --patient`, under 1800 s and chosen by name
  (2026-10-22): one caught is killed; one whose tests all pass is equivalent, only slower,
  and excluded with that reason; one still timing out fails the run. Stopping a binary at
  its first failure needs a nightly harness flag, so a binary's remaining tests run on
  after one fails, which is what outruns the timeout.
- **The build** (2026-11-10): `mutants.py` gives cargo-mutants
  `--cargo-arg=--config=profile.test.opt-level=1`, so that every cargo command of the run,
  each mutant's and the baseline's, builds the tests at opt-level 1, `Cargo.toml`'s
  profiles untouched; debug assertions, and the overflow checks that follow them, stay on.
  The verdict fails on a report whose baseline's tests were not so built, its timeouts
  being set by an unoptimized suite. Unoptimized, about 80 % of the tests' time in the
  run on `f58a490` went to binaries that passed before the one that caught the mutant,
  and four of colino's and brodo's slowest binaries took half of it. A trial of 106
  mutants on GitHub Actions took 485 job-minutes unoptimized, 143 at opt-level 1, 147 at
  3, and 448 under cargo-nextest stopping at the first failure (ADR-0034), each mutant's
  outcome the same. The whole run at opt-level 1 on `78def09`, against the run on
  `3a97708` with no code between them: 4665 mutants, none missed in either, eight moved
  between caught and timed out and none else; 164 min against 619, 2468 job-minutes
  against 7849, the longest shard 45 min against 331. Building optimized doubles the
  build's time, 561 to 1077 job-minutes. cargo-nextest stays a choice of `mutants.yml`.
- **The verdict** is the last report's missed mutants and the patient run's, and fails on
  a missed mutant, a timeout before any test failed, a baseline that failed or is missing,
  a list of none, and an argument it does not know (2026-11-02). Reports given with `--`
  and the run's arguments must report every mutant `cargo mutants --list` gives, naming
  how many are in none (2026-10-27: two shards cut off at six hours went unseen); a shard
  with no `outcomes.json` and an empty `mutants.json` reports none, and one with mutants
  listed and no outcomes is a lost report (2026-10-28). A report of mutants run needs the
  guard's log beside it, and a patient run unparted reports every timeout it was given.
- **Exclusions.** A missed mutant is killed by a new test or excluded in
  `.cargo/mutants.toml`'s `exclude_re`, a regex without line numbers under a comment
  giving its reason and ending in its kind, `[same]`, `[open: WHERE]` or
  `[unreached: WHERE]`, an equivalence under the contract, not the implementation
  (ADR-0043, 2026-10-28). Not `#[mutants::skip]`, which needs the `mutants` crate as a
  regular dependency. The gate (`mutants.py` alone) fails on an exclusion without a
  comment or a kind, a WHERE naming no document or item (an item found in its type's
  definition and `impl` blocks alone), and one no longer excluding a mutant by
  cargo-mutants' own regex engine.
- **The exclusions file is a rule file** (2026-10-28): AGENTS.md lists it, and every
  exclusion is approved by the human, its date listed below. It holds the exclusions alone,
  read by its shape: one `exclude_re = [` line, its patterns and comments, a `]`, and
  outside them blank lines and comments (2026-10-18, 2026-11-02); the run's settings live
  in `mutants.py`, so that none changes without an ADR.
- **A property test** kills a mutant only as soundly as it is steady: a run follows a gate
  in which every test passes, and a property test failing in a run is investigated before
  the run is trusted (2026-10-25).

## Consequences

- Closing a milestone takes hours more, run at rest or on GitHub Actions in shards
  (ADR-0034). Nothing checks that the run happened; the milestone's checklist lists it.
- A mutant that only exhausts memory counts as caught, since the guard's kill fails the
  test binary. Doctests run through rustdoc, outside the guard.
- A mutant that slows the tests of one binary and breaks only a later one's can time out
  first; a test of it in a binary run earlier, the crate's unit tests, fails it in seconds
  (P9).

## Exclusions approved

Each by the human, on the date given, the reasons in `.cargo/mutants.toml`; those since
gone with their code were removed with it.

- 2026-10-22: P2's mutants found only slower by the patient run, 1 of brodo's and 14 of
  colino's.
- 2026-10-25: three in `colino::intersect`.
- 2026-10-27: twelve of P5's; three exclusions withdrawn, their equivalence or code gone.
- 2026-10-28: nine left by the final run of P5's fixes.
- 2026-10-31: two of P6's.
- 2026-11-01: six of P7's `.vdb` reader and writer.
- 2026-11-02: those of `colino::distance::on_triangle`'s thin triangles, as `[open]`, its
  documentation saying which point within rounding or a thin triangle's width is given is
  not promised; and four of the tree's margins and two of the nearest query's, only
  slower, left by 0.8.2's run, seven no longer matching removed.
- 2026-11-04: six of P8's, as `[same]`.
- 2026-11-07: eight of P9's: five of a frozen buffer's root checks and a cache file's
  bounds check as `[open]`, and three of a `.vdb` reader's widening of `f64` as `[same]`;
  `cache::read`'s documentation then saying which of several broken rules is named is not
  promised.
- 2026-11-11: six of P12's, as `[same]`: four of the root's empty regions in labelling,
  one of them only slower, and two of a `.vdb` reader's default for a value's distance.
