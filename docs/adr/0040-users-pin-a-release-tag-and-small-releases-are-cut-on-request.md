# ADR-0040: Users pin a release tag, and small releases are cut on request

- **Status**: Amended
- **Date**: 2026-10-25

## Context

ARCHITECTURE.md §7 said users pin a git revision; no README said how to depend on brodo.
TESTING.md V2 raises MINOR when a milestone closes and PATCH for a fix of the last version
only. A user that reaches brodo both directly and through another crate, in one process,
gets two distinct copies of every brodo type when the two pins differ, and the copies do
not mix: a `Topology` from one is not a `Topology` of the other.

## Decision

The owner chose, on 2026-10-25, the recommended option of each point:

- **a. What users pin.** (1) A release tag, `tag = "vX.Y.Z"`, said in the README and
  ARCHITECTURE.md §7. (2) Any revision, as now. *Recommended: (1)*.
- **b. A feature a user needs before the next milestone closes.** (1) A small release is
  cut: MINOR raised between milestones when the change is additive or breaking, its section
  in CHANGELOG.md, the gate passed, tagged; V2 amended to allow it. (2) The user pins the
  revision meanwhile. (3) The user waits for the milestone. *Recommended: (1)*: every user
  then names a tag, and two crates in one process can agree on one.
- **c. What a small release checks.** (1) The gate only; the milestone's closing checks
  wait for the milestone (ADR-0033). (2) The closing checks too. *Recommended: (1)*.

## Consequences

A user depending through another crate asks that crate to move to the same tag; the README
says why. Registration on crates.io, where versions resolve without tags, stays a later
decision (ROADMAP.md §4).

## Amendment: a GitHub release for each tag

- **Date**: 2026-10-31

No version up to 0.6.0 had a GitHub release; a tag and its CHANGELOG.md section were the
release. The human decided that 0.7.0 gets one, the versions before it none, and every
version after it one too: made once its tag is pushed, titled with the version, its notes
the version's CHANGELOG.md section as it stands, as proposed. TESTING.md V4 says so. The
gate runs before the tag is pushed, so nothing checks that the release is made.

## Amendment: depending by version, and publishing, from 1.0

- **Date**: 2026-11-07

The crates go to crates.io at 1.0.0 (ADR-0011's amendment of 2026-11-03), where cargo
resolves every crate of a program within one MAJOR to one copy, which a pinned tag had
given before. The owner decided, on 2026-11-07, as proposed:

- **What users depend on.** From 1.0, a version from crates.io, `brodo = "1"`, in place of
  a tag (decision a). README.md says so once 1.0 is published, and says to pin a tag
  until then. A small release on a user's request stays (decision b), raising MINOR, or
  PATCH for a fix alone (TESTING.md V2).
- **Publishing.** From 1.0, each release is published to crates.io once its GitHub
  release is made: brodo, then colino, then mestolo, each depending on the one before. The
  owner runs `cargo publish` on the owner's machine, since publishing takes the owner's
  token and cannot be undone (crates.io yanks a version but never deletes it); an agent
  runs `cargo publish --dry-run` alone (TESTING.md V4). Nothing checks it.
