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

## Amendment

- **Date**: 2026-11-01

P7's retrospective found Rule 8's grep failing `Result<BTreeMap<String, String>, VdbError>`,
a typed error whose value type holds strings, so that the code took a type alias to pass.
The owner had the grep match `String` only as `Result`'s own error, its value type nested at
most two deep (`scripts/audit/verify_compliance.sh`). Shown to fire on sample lines: the new
pattern matches `Result<(), String>`, `Result<Vec<u8>, String>` and
`Result<BTreeMap<u32, Vec<u8>>, String>`, as the old one did, and no longer
`Result<BTreeMap<String, String>, VdbError>` or `Result<Vec<String>, VdbError>`; a value type
nested deeper is not matched.

## Amendment: Rule 15 leans on `cargo fmt`

- **Date**: 2026-11-02

The review of the whole repository found `scripts/audit/source.py`'s call graph reading a
function's body from the line after its signature, so that a recursive function written on
one line passed. `cargo fmt`, held by Rule 10, never leaves a body on its signature's line,
and the same function formatted failed. The owner decided, on 2026-11-02, to keep the check
as it is and state that it leans on `cargo fmt`, in CODING.md Rule 15 and AUDITING.md.

## Amendment: production code read up to the test module

- **Date**: 2026-11-02

The review of the whole repository found `scripts/audit/source.py` taking a file's
production code to end at its first line reading `#[cfg(test)]`, indented or not, so that a
test-only item before the test module ended it early: in `bvh.rs`, `accessor.rs`, `tree.rs`
and `topology.rs`, about 940 lines of production code went unread by the checks on asserts,
recursion and the lints an exception names; none held anything they would refuse. It also
found the gate's step titles naming neither Rule 18, which `source.py` checks, nor L2, which
`paths.py` checks. The owner decided, on 2026-11-02, that production code ends at an
unindented `#[cfg(test)]` followed by `mod`, a test-only item before it read as production
code, and that the titles name both. Shown to fire: an assert added to `tree.rs` after its
first `#[cfg(test)]` passed the old check and failed the new one.

## Amendment: Rules 8 and 9 read as code, and the test module by its name

- **Date**: 2026-11-02

The second review of the whole repository found the gate's greps for Rules 8 and 9 passing
`Result<T, std::string::String>`, a `Result` whose types nest three deep or break over
lines, `filter_map(|r2| r2.ok())` and `filter_map(std::result::Result::ok)`; that
`source.py` took production code to end at an unindented `#[cfg(test)]` before any `mod`,
where only `mod tests` is the test module; and that Rule 15's text named closures and trait
objects as what the call graph misses, though it misses a method called on another receiver
or by a path too. The owner decided, on 2026-11-02, that `source.py --errors` checks Rules 8
and 9 on each file's code, its comments cut, matching a `Result`'s brackets and reading
across lines; that production code ends at `#[cfg(test)]` followed by `mod tests`; and that
Rule 15's text names what the graph misses, since counting every `.name(` as a call joined
each function to the methods of its name on other types and made 21 cycles, 18 of them of
one function alone, such as `len` calling a `Vec`'s `len`. Shown to fire: a file holding
those four forms passed the greps and failed `source.py --errors` at each.

## Amendment: a string before a comment, a lint named `reason`, and an arrow

- **Date**: 2026-11-02

An outside review of the uncommitted checks found `source.py` cutting each line at its
first `//` before emptying its strings, so that code after `"http://…"` on its line, a
`Result<u8, String>` or an `unsafe`, went unread; skipping every part of an exception
holding the word `reason`, so that an `#[expect]` of `clippy::allow_attributes_without_reason`
passed Rule 19; and taking the `>` of `->` for a bracket, so that `Result<fn() -> usize,
String>` passed Rule 8. It also reported that a second unsafe operation in the block of
`unsafe { CacheFile::map(` passed Rule 4, which clippy's `multiple_unsafe_ops_per_block`,
denied in every target, refuses. The owner decided, on 2026-11-02, that all of it be fixed,
and it was: each line's strings and characters are blanked before its comment is found, in
every check; an exception's part is skipped as its reason when it holds `=`; and the `>`
of `->` and `=>` closes no bracket. Shown to fire: a file holding each of the three passed
the old checks and failed the new, which pass the repository.
