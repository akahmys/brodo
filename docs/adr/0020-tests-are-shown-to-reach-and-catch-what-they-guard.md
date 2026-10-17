# ADR-0020: Tests are shown to reach the cases they are meant for, and to fail where they guard

- **Status**: Accepted
- **Date**: 2026-10-17

## Context

P1's retrospective found two ways a passing test had checked less than it seemed to.
Property tests were found, more than once, whose generated cases never reached the input
they were written for, so they passed having tested nothing of it; each was found only
when someone counted the cases. And a T3 break meant to make the `.vtu` writer repeat
points made it fail with an error before the assertion on repeated points ran: the test
failed, but not because it caught the defect.

## Decision

- TESTING.md T9: the commit that adds or changes a property test counts, by temporary
  instrumentation, how many cases reached each kind of input the test names, and states
  the counts in its message.
- TESTING.md T3: the break must fail the test at the assertion that guards the defect,
  not at an error or panic elsewhere.

## Consequences

Both are held by review alone: nothing checks a commit message's counts or where a T3
break failed. A check that counted cases inside every property test was not adopted,
being heavier than the defect it guards.
