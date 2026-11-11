# ADR-0011: Milestones are the project group's phases, and versions are recorded in CHANGELOG.md

- **Status**: Amended
- **Date**: 2026-10-16

## Context

The template numbered milestones M0, M1 and listed versions in a table in ROADMAP.md.
brodo carries the phases P0 to P2 of a larger plan, and its users pin it by git revision
and need to see what each version breaks. The
grid layout and the file formats outlive any one crate version.

## Decision

- Milestones are named after the phases brodo carries (P0, P1, P2), and later work is a
  milestone once the human schedules it.
- CHANGELOG.md replaced ROADMAP.md's release table: one section per version, newest first,
  with the milestones closed and every breaking change (TESTING.md V3).
  `scripts/audit/workspace.py` reads the version from it.
- The `GridView` layout and each file profile carry their own format version (V6).

## Consequences

Breaking changes are allowed before 1.0, but never silently.

## Amendment: P8, P9 and 1.0 on crates.io

- **Date**: 2026-11-03

After 0.8.2, the units left after P7's review were listed in ROADMAP.md as work of no
milestone, so that TESTING.md V2 gave them no version. B-023 and B-022 were among them, and
both raise a format version (ADR-0038). Registration on crates.io had been left to a later
decision (ADR-0040), and 1.0 is the owner's (TESTING.md V5).

The owner decided, on 2026-11-03:

- **P8** is those units but the reading of an archive's region from its chunks' bytes,
  which is left to later: the bound on what reading a `.vdb` makes (ADR-0047), the command
  to look inside brodo's files, README.md's "Looking inside" section, and B-023 with
  B-022. It closes as 0.9.0.
- **P9** prepares brodo and colino for crates.io, where both are first published as 1.0.0:
  the formats settled by P8 before any stability is promised, then the public API reviewed
  for what would break a user, the rules for versions after 1.0, and the packages.

Publishing at 1.0.0 rather than at 0.9.0 was chosen because a first release on crates.io
is the first a user resolves by version alone, and it should already promise the API and
the formats.

The decision on 1.0 itself is still an ADR of its own when P9 closes (V5); the rules P9
changes are each changed through the ADR on their topic, with the owner's approval.

## Amendment: versions after 1.0

- **Date**: 2026-11-07

TESTING.md V1 and V2 were written for versions before 1.0, `0.MINOR.PATCH` with a MINOR
free to break the API, and no rule said when `rust-version` may rise. P9 publishes the
three crates to crates.io as 1.0.0 (the amendment above), where cargo resolves versions by
semantic versioning. The owner decided, on 2026-11-07, as proposed:

- **One version for the three crates.** brodo, colino and mestolo keep the one workspace
  version (V1), released together; separate versions were not taken now. Its cost: a
  release breaking only colino's or mestolo's promises raises brodo's MAJOR too. Moving
  to separate versions later breaks no user, and separate versions now would let colino's
  stated dependency on brodo fall behind the brodo it builds against in the workspace,
  unless a check of its least versions were added; colino's API carries brodo's types, so
  that its MAJOR follows brodo's whatever is chosen. V1's form, `MAJOR.MINOR.PATCH` with
  no pre-release or build part, is checked by `scripts/audit/workspace.py`; cargo itself
  refuses a version that is not semantic versioning's, such as `1.0`.
- **Semantic versioning from 1.0** over the promises: those of AGENTS.md's three homes and
  of `mestolo`'s command line and output as ARCHITECTURE.md §6.3 states them, a hidden
  item promising nothing. A release breaking one raises MAJOR, adding to them MINOR, only
  fixing the last version PATCH; a milestone's close raises MINOR, or MAJOR if it breaks
  one (V2). Whether a release breaks one is checked by V9 (the amendment below).
- **`rust-version`** rises only in a release raising MAJOR or MINOR, its CHANGELOG.md
  section saying so, as most crates on crates.io treat it; a MAJOR for each rise was
  judged too heavy. A version whose PATCH is above 0 is checked to state the
  `rust-version` of its MINOR's tag, `vMAJOR.MINOR.0`, read from git (V7, a new rule).

Shown to fire (TESTING.md T3, 2026-11-07): the workspace version set to `1.0.0-rc.1` and
to `0.9.0+build.5` failed V1's check; `0.9.1` with `rust-version` raised to 1.99 failed
V7's against tag `v0.9.0`, and with it unchanged passed it; `0.10.1`, whose tag `v0.10.0`
does not exist, failed V7's. `1.0` and `0.09.0` were refused by `cargo metadata` before
the check ran.

## Amendment: a release checked against the last one

- **Date**: 2026-11-07

V2 promised semantic versioning from 1.0 with nothing to check it. cargo-semver-checks
compares a library's rustdoc JSON with a baseline's. Measured on 2026-11-07: rustc 1.98.1,
the pinned toolchain, writes rustdoc JSON format 60, which cargo-semver-checks reads from
0.49.0 on; 0.51.0, the newest, needs Rust 1.93 and ran on the three crates, in 26 s with
nothing built and 3.6 s built, against tag `v0.9.0`. There it reported the breaks since
0.9.0, all intended, and one CHANGELOG.md had missed: colino's `ReadError` no longer
`UnwindSafe` nor `RefUnwindSafe`. Against a baseline it had just been compared with, an
item made private, a generic parameter added, a trait method added, a variant added to an
exhaustive enum, a public field made private, an auto trait lost, a feature removed, a
feature dropped from `default` and an item made `#[doc(hidden)]` each failed; a field's,
an argument's or a return's type changed, a bound added to a function or a struct, a
lifetime narrowed to `'static` and a constant's value changed, `mestolo::USAGE`'s among
them, did not, nor did an item moved behind a feature, under every feature or the tool's
default; under `default` features it did. A hidden item removed passed, as V2 has it
promise nothing. A version equal to the baseline's is judged a MINOR, and one of 0.x
raising MINOR a MAJOR.

The owner decided, on 2026-11-07, as proposed:

- **V9**: from 1.0, the gate compares each crate's library with the newest release tag
  merged into HEAD, by cargo-semver-checks 0.51.0, with no feature, each feature alone and
  every feature, as clippy's Rule 10 builds them, so that an item moved behind a feature
  is seen. A tag rather than the version on crates.io: it needs no network, and the
  package holds the tag's `src/` (V8).
- **`mestolo`'s command line and output**, which no rustdoc shows, are held by the
  baseline's own tests, `crates/mestolo/tests/` at the tag, run on the current code: the
  tests assert the promises (T12), so a release passing the last one's keeps them.
- **A break for the next MAJOR** is declared by `### Breaking` under CHANGELOG.md's
  `## Unreleased`, where V3 already puts every break, which suspends the check until that
  version; a version whose section has `### Breaking` must raise MAJOR. The check is
  skipped before 1.0, where a MINOR may break the API, and over a MAJOR raised, saying why.
- **What the tool misses**, measured above, is held by review, and by the last release's
  tests where they hold it.
- The tool's version is pinned in `scripts/audit/semver.py`, which fails when another
  runs or `audit.yml` does not install it, and stated in TESTING.md §3 and README.md.

Shown to fire (TESTING.md T3, 2026-11-07), in a scratch clone tagged `v1.0.0` with a
probe module: a public function made private failed, under every feature set; a function
moved behind `vtk` failed with no feature and each other feature alone; `mestolo`'s exit
status for a refused command line changed from 2 to 3 failed in the tag's tests, also with
the current tests changed to agree; `### Breaking` under `## Unreleased` skipped the
check; 1.1.0 whose section has `### Breaking` failed; 2.0.0 skipped it; the pin set to
0.50.0 failed on the tool and on `audit.yml`; with tag `v1.0.0` removed, 1.0.0 over
`v0.9.0` skipped it. Unchanged, it passed, in 2 min with nothing built and 20 s built.

## Amendment: a baseline test set aside

- **Date**: 2026-11-10

Issue 3 had `.vdb` hold vector grids (ADR-0046, ADR-0047). Five of v1.2.0's mestolo tests
asserted that `convert` refused or skipped a vector grid as `.vdb` and that `info` listed a
`.vdb`'s vector grid as skipped, so V9 failed on 1.3.0's code, which adds what they held
absent: a MINOR under V2, not a break of a promise, but V9 could not tell them apart.

The owner decided, on 2026-11-10, as proposed: `scripts/audit/semver.py` holds a table,
`SET_ASIDE`, of the baseline's mestolo tests not run, each by its tag and test name, with
why. They are skipped by exact name for that tag alone; an entry of another tag, or naming
no test of its tag, fails the check, so that an entry does not outlive its baseline and a
misspelt one is found. A break, rather than an addition, still declares `### Breaking`.

Shown to fire (TESTING.md T3, 2026-11-10): an entry's tag changed to `v1.1.0`, its name
misspelt, an entry removed, and an extra entry of no test or of another tag each failed;
unchanged, it passed against `v1.2.0` in 35 s.
