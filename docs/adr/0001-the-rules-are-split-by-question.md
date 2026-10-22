# ADR-0001: The rules and the design are split by the question each document answers

- **Status**: Amended
- **Date**: 2026-10-16

## Context

One document answering many questions is where facts get stated twice and drift apart,
and agents forget rules that sit far from the work they govern. A status file updated
every session, kept apart from the plan, drifted from it.

## Decision

- Each document answers one question, and AGENTS.md lists them and what outranks what.
  The rules are split by the phase of work they govern (planning, coding, testing,
  auditing); the design is in ARCHITECTURE.md, the plan and the status in ROADMAP.md, and
  the reasons in these records.
- The status is ROADMAP.md §1, overwritten at the end of each unit of work, so that
  status and plan are read in one place.
- Documents, code and commit messages are in English. Rules state what enforces them.
- A rule changes only with the human's approval and a record here, in one commit with its
  enforcement.

## Consequences

The design changes only by ADR. `scripts/audit/documents.py` checks the links, the
language, the tense of the present-tense documents and this index; the `commit-msg` hook
requires an ADR for a commit that changes a rule.

## Amendment

- **Date**: 2026-10-17

P1's retrospective found crate docs pointing at ARCHITECTURE.md §3 and §4 for designs in
§4 and §5, which no check had caught: `documents.py` resolved Markdown links but not
section references. It was made to fail on a `NAME.md §N` or `§N.M`, in a document or a
source comment, that names a root document without a heading so numbered; it checked 107
such references when added. Changing a reference in `vtk.rs` from §6.1 to §6.9 failed it.


## Amendment

- **Date**: 2026-10-20

colino was readied for publishing on its own (ROADMAP.md, P2), and a crate needs a README
for its users, which no document answered. The human decided:

- A crate's README answers what the crate does and how it is used, and holds no rule or
  design; AGENTS.md lists it.
- It is the crate's documentation front page, read in by `include_str!` in a `#![doc]`
  attribute of its `lib.rs`, so that its code runs as a doctest in the gate's
  `cargo test` and cannot drift from the API. That a README is read in so, nothing
  checks.

Shown to fire: an assertion in colino's README changed to a wrong value failed as a
doctest of `cargo test --workspace --all-features`, and passed again once restored.

## Amendment

- **Date**: 2026-10-20

P2's retrospective found two ADRs whose claims about the code were wrong, each found only
while implementing. ADR-0028 had the complement's Betti numbers counted, not derived, but
under the 26/6 pairing each lattice vertex adds as much to the complement's χ as to the
solid's, so only its β₀ could be counted on its own. ADR-0029 had no section of the frozen
layout change shape, but the header held the background in twelve bytes, too few for
`[f64; 3]`. The human decided:

- PLANNING.md §2: an ADR's claims about the code as it is are established by running
  something before the human decides; one found wrong while implementing goes back to the
  human as an amendment before the work lands. Held by review alone.

## Amendment

- **Date**: 2026-10-22

brodo gained a README at the owner's request, as colino had: what it does and how it is
used, read in by its `lib.rs`, its code a doctest. The human decided that AGENTS.md lists
it beside colino's. Shown to fire: the README's Euler characteristic changed to a wrong
value failed as a doctest of `cargo test -p brodo --doc`, and passed again once restored.

## Amendment

- **Date**: 2026-10-22

P3's retrospective found ADR-0030's claim that every crossed edge's owner is a leaf
measured on well-made meshes only; a cube with one face turned in broke it. The human
decided that a claim holding for every input is measured on each kind of input the design
accepts, a badly oriented, holed or degenerate mesh among them (PLANNING.md §2). Held by
review alone.

## Amendment

- **Date**: 2026-10-22

P3's retrospective found that brodo had no README until asked for one, and that nothing
would have said so: AGENTS.md held that a README is read in, but nothing checked it. The
human decided that `scripts/audit/documents.py` checks every crate in `LAYERS` for a
README.md and for the `include_str!` in its `lib.rs` that reads it in. Shown to fire:
with brodo's `lib.rs` line removed, and with colino's README moved away, the check failed
naming each; restored, it passed.

