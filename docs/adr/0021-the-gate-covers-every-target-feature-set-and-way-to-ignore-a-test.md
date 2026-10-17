# ADR-0021: The gate covers every target, every feature set, and every way to ignore a test

- **Status**: Accepted
- **Date**: 2026-10-17

## Context

Before P1 closed, the human asked whether the audit checks what it claims to. Three gaps
were found by trying them, each passing the gate: a test switched off by
`#[cfg_attr(…, ignore)]`, which the T1 step's search for `#[ignore` did not match; an
`#[expect(clippy::print_stdout)]` in a benchmark, since `source.py` read only `src/`; and
code behind a cargo feature, which was built and linted only with the default features.

## Decision

- TESTING.md T1: the search finds an ignore written directly or under `cfg_attr`, in
  `src/`, `tests/`, `examples/` and `benches/`.
- CODING.md Rule 19: `source.py` checks the lints an exception names in every target, not
  only in production code; asserts and recursion stay a production-code check.
- CODING.md Rule 10: the gate runs `cargo clippy --all-targets -D warnings` on each member
  with no default features and with each feature alone, besides the defaults.

Each was shown to fire: a test under `#[cfg_attr(all(), ignore)]`, an excepted
`print_stdout` in `benches/brodo.rs`, and a re-export of `slice` outside its feature each
failed the gate, which passed again once each was removed.

## Consequences

Combinations of two or more non-default features are not linted; with two features,
every combination is covered. The tests themselves run with the default features only.
