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
  outran the 431 s timeout each time. Stopping a binary at its first failure needs a
  nightly harness flag.

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

## Amendment

- **Date**: 2026-10-22 (slow mutants)

P2's run found mutants that leave every result as it was but make a search exhaustive: a
bounding-volume test that never prunes, a partition that never splits. Run again on a
machine at rest, 33 of colino's and 3 of brodo's still timed out before any test failed,
and T11 gave them no way to pass. The human decided:

- `mutants.py --run --patient` runs again, under a timeout of 1800 s, only the mutants the
  last report shows timing out before any test failed, chosen by name. One caught is
  killed. One whose tests all pass is equivalent, only slower, and goes into
  `.cargo/mutants.toml` with that reason. One still timing out fails the run. cargo-mutants'
  `--re` lets a deleted struct field through whatever its name, so only the named count.
- The run's verdict is the last report's missed mutants and the patient run's.

Shown to fire on P2's run: of brodo's 3, one passed only slower and two still timed out;
of colino's 33, 17 were caught, 14 passed only slower and two still timed out.

## Amendment

- **Date**: 2026-10-25

P3's full run counted as caught seven mutants of `colino::intersect` that no test could
kill: the property test of crossings fails at random on a case it happens to draw, and
until 2026-10-25 a real defect, a crossing at a segment's end rounded off, sometimes made
it fail under any mutant. With the defect fixed, the run of that file's mutants left the
seven missed. Four were killed by a unit test of ties at three sizes; the human decided
that three are equivalent and listed them with their reasons: the axis taken on a tie of
the largest components, `limit`'s sign where it cannot be zero, and a division by a zero
denominator left out by the range check. A mutant's kill by a property test is only as
sound as that test is steady: a run follows a gate in which every test passes, and a
property test that fails in a run is investigated before the run is trusted.

## Amendment: every feature

- **Date**: 2026-10-25

P4's run, on GitHub Actions in 60 shards at `63fcbe1`, missed 89 mutants in
`brodo/src/slice.rs` and 59 in `brodo/src/vtk.rs`: ADR-0036 had made `slice-png` and
`vtk` opt-in, and cargo-mutants, run with the default features, still mutated that code but
no longer built the tests behind those features. The gate's tests ran with every feature;
the run did not. The owner decided that `mutants.py` passes `--all-features`, as the gate's
`cargo test` builds, and TESTING.md T11 says so. Shown to fire: without it, the 148 were
missed; with it, run again on the owner's machine, 146 were caught and 2 were unviable.

The same run missed 7 mutants of `colino/src/hermite.rs` that no test reached: a triangle
with no area nearest an unhit edge, an edge outside at its lower end crossed more than once,
and a normal perpendicular to its edge. Three tests in `hermite_reference.rs` now build
each case, and caught all 7. Its 16 timeouts before any failed test, in `winding.rs` and
`bvh.rs`, ran again under 1800 s on the owner's machine and were all caught, the slowest
of them in about 13 minutes: GitHub's runners had made them outrun twice the unmutated
suite's time.

## Amendment: two exclusions withdrawn

- **Date**: 2026-10-27

P5's review rewrote `euler`'s sum over the tiles' edges (M1): each tile sums, in
parallel, the vertices on its edges that it is the first meeting tile to hold. Two
mutants had been excluded as equivalent, each leaving a tile's far edges unvisited, on
the ground that another tile touching such an edge visits it on its near side. That no
longer holds: the vertex is the first tile's to sum, and a small tile touching a large
one's far edge mid-way is lost. A test of that case kills both, and the human decided
that the two exclusions are withdrawn.

## Amendment: P5's missed mutants

- **Date**: 2026-10-27

The run on `9940bd5` (2026-10-27) missed 72 mutants, all in code P5 wrote or rewrote.
Most are now killed by tests written for them, each shown to fail with its mutant applied
by hand; the hand-applied runs went behind `rss_guard.py` once one, a voxel of no size let
through, grew until the machine went down. Dead code that held others is gone: the level-3
arm of the labelling's `classify` and the early return of its `inside`, a guard on a
section's height never met, the parity lines' growth of the box. The human decided that
twelve that no test can tell from the original are excluded, each with its reason, and
that the exclusion for `inside`'s early return, gone, is withdrawn.
