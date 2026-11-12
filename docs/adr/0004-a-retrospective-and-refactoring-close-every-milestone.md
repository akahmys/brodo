# ADR-0004: A retrospective and refactoring close every milestone

- **Status**: Amended
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

## Amendment: which checks a milestone closes with

- **Date**: 2026-11-02

From P5 on, the human chooses which of the closing checks this record holds a milestone
to, by its content, and the milestone's checklist lists them (ADR-0033).

## Amendment: never left out, and checked

- **Date**: 2026-11-12

P10, P11 and P12 closed without a retrospective, none of their checklists having listed
it, while in those days a release's tag left V9's set-aside tests stale and failed the gate
on GitHub after the push, two readers of the Hermite format agreed while both were wrong,
and a benchmark's drift was described as between processes when it was between sessions.
The owner decided, on 2026-11-12, that every milestone closes with the retrospective and
refactoring again, whatever closing checks it is given (ADR-0033), and that it be checked:
`scripts/audit/documents.py` fails where a milestone in ROADMAP.md §2 does not list
`The retrospective and refactoring (PLANNING.md §7)`, or ticks its version before it.
Whether the unit was done well stays held by review.
