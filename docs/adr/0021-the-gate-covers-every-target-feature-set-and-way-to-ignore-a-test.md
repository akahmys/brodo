# ADR-0021: The gate covers every target, every feature set, and every way to ignore a test

- **Status**: Amended
- **Date**: 2026-10-17

Condensed on 2026-11-08 (ADR-0001): each decision is kept with its reason and what it set
aside; the text before is in git, this file as of `53f69d5`.

## Context

Before P1 closed, the human asked whether the audit checks what it claims to. Three gaps
were found by trying them, each passing the gate: a test switched off by
`#[cfg_attr(…, ignore)]`, which the T1 step's search for `#[ignore` did not match; an
`#[expect(clippy::print_stdout)]` in a benchmark, since `source.py` read only `src/`; and
code behind a cargo feature, which was built and linted only with the default features.

## Decision

The human decided, on 2026-10-17 unless dated otherwise.

- **Every way to ignore a test** (TESTING.md T1). `source.py --ignores` finds an ignore
  written directly or under `cfg_attr`, across its lines, in every target, in the code
  alone with its strings blanked, a reason being the string an `ignore =` of the code
  opens (2026-11-02: a string spelling `#[ignore]`, and a `]` in a string before an
  `ignore`, had fooled the search).
- **Every target** (CODING.md Rule 19). The lints an exception names are checked in every
  file whole, written directly or under `cfg_attr`, test modules included; asserts and
  recursion stay a production-code check.
- **Every feature.** The features are additive, so all of them together is one more build,
  not one per combination. The gate runs with `--all-features` the tests (2026-10-20: a
  test behind colino's `serde` never ran), Rule 12's `cargo tree` (a dependency behind a
  feature could pull `cc` unseen), T2's list, the steps of Rules 2 and 3 (2026-11-02: three
  of the nine modules reading untrusted input sat behind features they did not build) and
  `cargo doc` with warnings denied (2026-11-02). Clippy runs on each member with no default
  features and with each feature alone, besides the defaults (Rule 10).
- **Doctests are listed** (2026-11-02), each by its file, its item and its place among the
  item's, not its line, which an edit above would change, and every crate with a list is
  held to it, so that a README's example cannot be deleted with no record (T2).

## Consequences

Combinations of two or more non-default features are not linted by clippy; built together
with every feature, they are by the tests and the other steps.
