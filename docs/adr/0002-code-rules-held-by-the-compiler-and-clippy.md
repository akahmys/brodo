# ADR-0002: Code rules are held by the compiler and clippy, and Power of 10 is adopted in part

- **Status**: Amended
- **Date**: 2026-10-16

## Context

Text-matching checks miss what the compiler and clippy catch, and a rule that nothing
checks is not followed for long. Agents also find the ways round a rule that only names
its intent.

## Decision

- Rules are enforced by rustc and clippy where they can be (`[workspace.lints]`,
  `clippy.toml`), and by scripts only where they cannot (CODING.md).
- Panicking calls are banned in all production code. Indexing and slicing are banned in
  the crates that read untrusted input, listed in `scripts/audit/project.py`.
- From Holzmann's "Power of 10": no recursion without a stated bound, a maximum count on
  every loop that iterates toward a condition, checked numeric casts, no float equality
  and no debug output. Assertion density was not adopted: a quota invites assertions that
  check nothing, and `assert!` panics, so it joined the ban on panics.
- The ways round the rules were closed: an exception is `#[expect]` with a reason, for
  named lints only; `rustflags` in a cargo configuration is refused and the audit runs
  with `RUSTFLAGS` empty; a crate that does not inherit the workspace lints fails.
- A clippy step limited to some lints or some crates warns rather than denies and fails
  if clippy does not finish: denied, a finding in one crate stopped the build of the
  crates above it, which were then never linted, and the step passed having checked
  nothing.

## Consequences

The gate is one command, the tests included. The project's own lists (layers, application
crates, crates that read untrusted input, targets) live in one file, so a new crate is
placed before it is built.

## Amendment

- **Date**: 2026-10-16

The ban on indexing and slicing moved from crates to the modules that parse untrusted
input (ADR-0009).
