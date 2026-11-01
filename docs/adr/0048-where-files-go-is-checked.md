# ADR-0048: Where files go at the root and where scripts lie is checked

- **Status**: Accepted
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
