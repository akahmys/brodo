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
