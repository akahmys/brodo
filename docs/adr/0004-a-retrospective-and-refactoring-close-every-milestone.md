# ADR-0004: A retrospective and refactoring close every milestone

- **Status**: Accepted
- **Date**: 2026-10-16

## Context

The same kinds of error recur unless something collects them: claims written from memory
and later corrected by measurement, checks that pass having checked nothing, and
procedures redone. Duplication also builds up across units of work.

## Decision

- A unit of work that is a retrospective, then refactoring, is held before every
  milestone closes and whenever a unit leaves duplication behind (PLANNING.md §7).
- The retrospective turns each corrected claim, vacuous check and redone procedure into a
  proposal, which the human decides as any rule change.
- Refactoring changes no behaviour: tests stay as they are and the gate passes before and
  after.

## Consequences

Each milestone's checklist in ROADMAP.md carries the item; nothing else enforces it.
Lessons that are not decided into a rule or a check leave no record beyond git.
