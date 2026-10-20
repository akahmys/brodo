# ADR-0011: Milestones are the project group's phases, and versions are recorded in CHANGELOG.md

- **Status**: Accepted
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
