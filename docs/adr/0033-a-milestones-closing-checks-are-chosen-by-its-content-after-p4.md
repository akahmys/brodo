# ADR-0033: A milestone's closing checks are chosen by its content after P4

- **Status**: Accepted
- **Date**: 2026-10-23

## Context

Every milestone closed with a retrospective and refactoring (ADR-0004), a fuzz round
(TESTING.md T6), a Miri run (T7), benchmarks against their baselines and a mutation run of
every mutant (T11). P3's mutation run alone was expected to take about 25 hours on two
cores (2026-10-23, from P2's runs). P4 was scheduled as a review of the public API, which
changes the API's shape rather than what it computes, and P5 as several units of differing
kinds, some touching no parser, no unsafe code and no benchmarked path.

## Decision

The human decided, on 2026-10-23:

- Through P4, every milestone closes with all of these checks, as before.
- From P5 on, the human chooses which of them a milestone closes with, by its content,
  when it is scheduled or before it closes; its checklist in ROADMAP.md lists those chosen.
- The gate (TESTING.md §4) is unchanged, and every change still passes it.

## Consequences

T6's, T7's and T11's runs, and PLANNING.md §7's retrospective, hold at a milestone's close
as its checklist lists them from P5 on. Held by nothing: the checklist is reviewed by the
human, as the milestone's acceptance is.
