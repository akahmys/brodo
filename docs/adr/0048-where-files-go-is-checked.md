# ADR-0048: Where files go at the root and where scripts lie is checked

- **Status**: Amended
- **Date**: 2026-11-02

## Context

CODING.md L3 (what the root holds) and L4 (where a script lives) were held by review
alone. The review of the whole repository found each rule's history: L4 broken once, by
`scripts/formats/`, found by a review and moved (a148f19); L3 never recorded. A check of
L3 run while writing it found four files at the root that nothing reads,
`p_cache_v1.bin.txt`, `p_frozen_v1_f32.bin.txt`, `p_frozen_v1_f64x3.bin.txt` and
`p_hermite_v1.bin.txt`, committed with docs/formats.md (298bc37), and three entries
ARCHITECTURE.md §3's tree did not name: `.gitignore`, `.gitattributes` and `.github/`.

## Decision

The owner decided, on 2026-11-02, that `scripts/audit/workspace.py` checks both rules,
beside L1, over the files git tracks or would add, so that a new one fails before it is
staged:

- **L3**: every entry at the root is named in ARCHITECTURE.md §3's tree, which is the one
  list of what the root holds; a new entry is added there first.
- **L4**: a script, a `.py` or `.sh` file or an executable one, lies under `scripts/audit/`,
  `scripts/hooks/`, `xtask/` or `spikes/`, a spike's own scripts being part of it
  (PLANNING.md §3).

§3's tree was given the three entries it lacked. Shown to fire: a directory `notes/` at the
root, a `.py` file and an executable file under `docs/` each failed the step by name; a
`.py` file under a spike passed.

## Consequences

A new root file or directory is a change to ARCHITECTURE.md §3, and so to the design
(PLANNING.md §2). Where output goes (L2) stays unchecked.

## Amendment: each test writes in a directory of its own

- **Date**: 2026-11-17

P13's retrospective found a test failing for a reason its code did not show: two of
mestolo's tests, run in parallel, wrote the same file, `vectors.cache`, in
`CARGO_TARGET_TMPDIR`, which every test of a crate shares, and one read the other's. Twelve
test files wrote there, each naming its files its own way, by a literal, a helper or
`format!`, and the names of the committed files they read recur, so no check of the names
could tell a shared file from a read one.

The owner decided, on 2026-11-17, as proposed, that the sharing is made impossible rather
than looked for (TESTING.md T15): one file, `crates/brodo/tests/support/scratch.rs`,
included by `#[path]` into each test file that writes one, gives the running test its own
directory, `CARGO_TARGET_TMPDIR`, then its test target's name, then its own. The test
harness names each test's thread after the test, its module path joined by `::`, with one
thread or many, which a throwaway crate showed on 2026-11-17; `::` is written `-`, which
every system's paths take. `scripts/audit/source.py --ignores` fails a file of any target
but that one naming `CARGO_TARGET_TMPDIR` or calling `temp_dir(`, so that a test cannot
reach the shared directory by the two ways a test has; one writing by a path built
otherwise is held by review. The twelve files were changed to it, their tests unchanged
in what they check. Shown to fire: a function joining `CARGO_TARGET_TMPDIR` added to
colino's `obj_format.rs`, and one calling `std::env::temp_dir()` to mestolo's
`commands.rs`, each failed the step by file and line.
