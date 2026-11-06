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
  one (V2). Whether a release breaks one is checked by nothing yet (ROADMAP.md P9's
  cargo-semver-checks item).
- **`rust-version`** rises only in a release raising MAJOR or MINOR, its CHANGELOG.md
  section saying so, as most crates on crates.io treat it; a MAJOR for each rise was
  judged too heavy. A version whose PATCH is above 0 is checked to state the
  `rust-version` of its MINOR's tag, `vMAJOR.MINOR.0`, read from git (V7, a new rule).

Shown to fire (TESTING.md T3, 2026-11-07): the workspace version set to `1.0.0-rc.1` and
to `0.9.0+build.5` failed V1's check; `0.9.1` with `rust-version` raised to 1.99 failed
V7's against tag `v0.9.0`, and with it unchanged passed it; `0.10.1`, whose tag `v0.10.0`
does not exist, failed V7's. `1.0` and `0.09.0` were refused by `cargo metadata` before
the check ran.
