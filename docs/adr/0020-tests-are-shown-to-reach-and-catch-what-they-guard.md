# ADR-0020: Tests are shown to reach the cases they are meant for, and to fail where they guard

- **Status**: Amended
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

## Amendment

- **Date**: 2026-10-20

P2's retrospective found two T3 breaks that showed less than they seemed to. One rewrote
an expression into another that computed the same value, `chi - solid_euler` as
`solid_euler - chi + 2 * (chi - solid_euler)`: nothing was broken, and nothing failed.
Another made `cache_format` fail, and `cargo test` stopped there, so whether
`frozen_format` caught the same break was not seen until it was run alone. The human
decided:

- TESTING.md T3: the break is checked to change what the code computes, and each test
  target it may affect is run on its own.

Held by review alone, as the rest of T3 is.

## Amendment

- **Date**: 2026-10-22

P3's retrospective found a property test of `colino::intersect` meant for products that
round equal reached none in 4096 cases: drawn at random, such a case almost never comes. It
was rebuilt to construct them, by Bezout coefficients, and reached 3563. The human decided
that a kind of input reached by under 1 % of a test's cases is built by its generator,
not left to chance (TESTING.md T9). Held by review alone, as the rest of T9 is.

