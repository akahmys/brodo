# ADR-0002: Code rules are held by the compiler and clippy, and Power of 10 is adopted in part

- **Status**: Amended
- **Date**: 2026-10-16

Condensed on 2026-11-08 (ADR-0001): each decision is kept with its reason and what it set
aside; the text before is in git, this file as of `53f69d5`.

## Context

Text-matching checks miss what the compiler and clippy catch, and a rule that nothing
checks is not followed for long. Agents also find the ways round a rule that only names
its intent.

## Decision

The owner decided, on 2026-10-16 unless dated otherwise.

- **Who checks.** Rules are enforced by rustc and clippy where they can be
  (`[workspace.lints]`, `clippy.toml`), and by scripts only where they cannot (CODING.md).
- **Panics.** Panicking calls are banned in all production code. Indexing and slicing are
  banned in the modules that parse untrusted input (ADR-0009), listed in
  `scripts/audit/project.py`.
- **Power of 10**, from Holzmann: no recursion without a stated bound, a maximum count on
  every loop that iterates toward a condition, checked numeric casts, no float equality
  and no debug output. Assertion density was not adopted: a quota invites assertions that
  check nothing, and `assert!` panics, so it joined the ban on panics.
- **The ways round closed.** An exception is `#[expect]` with a reason, for named lints
  only; `rustflags` in a cargo configuration is refused and the audit runs with
  `RUSTFLAGS` empty; a crate that does not inherit the workspace lints fails.
- **A partial clippy step** warns rather than denies, and fails if clippy does not finish:
  denied, a finding in one crate stopped the build of those above it, never linted, and
  the step passed having checked nothing.
- **Rust read as rustc reads it** (2026-11-02). The checks clippy cannot make
  (`scripts/audit/source.py`) read each file through a lexer of their own, with no
  dependency added: line and nested block comments, raw, byte and C strings and
  characters told from lifetimes are blanked whole, line ends kept; attributes are read
  with their brackets matched, every part of every `allow` and `expect` held, one that is
  neither a lint nor `reason = …` failing. Reading by regular expressions had been patched
  one reported form at a time, a string's `//`, a raw string's quote, a `cfg_attr` holding
  two `expect`s, each passing what it should refuse. The files read are those git would
  commit. Production code ends at `#[cfg(test)]` followed by `mod tests`, a test-only item
  before it read as production code (the first line reading `#[cfg(test)]` had hidden
  about 940 lines).
- **Rules 8 and 9** (2026-11-02) are checked on each file's code by `source.py --errors`, a
  `Result`'s brackets matched across lines, `->` and `=>` closing none: Rule 8 refuses an
  error that is a string, owned, borrowed, boxed or copied on write, or a boxed
  `dyn Error`, at any depth of the value type; Rule 9 by `flatten`, which needs the
  iterator's item type, is left to review. The greps it replaced passed a fully named
  `String`, a nested or broken `Result` and `filter_map(…ok)`.
- **Rule 15's call graph leans on `cargo fmt`** (2026-11-02): it reads a body from the line
  after its signature, which `cargo fmt` (Rule 10) always gives. It misses closures, trait
  objects and a method called on another receiver or by a path: counting every `.name(` as
  a call joined each function to every method of its name and made 21 false cycles.

## Consequences

The gate is one command, the tests included. The project's own lists (layers, application
crates, modules that read untrusted input, targets) live in one file, so a new crate is
placed before it is built.
