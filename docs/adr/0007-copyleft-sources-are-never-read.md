# ADR-0007: Copyleft and sibling sources are never read, and every source file records what it drew on

- **Status**: Amended
- **Date**: 2026-10-16

## Context

brodo is permissively licensed and is used beside copyleft code, some of it built on
OpenFOAM's ideas. A port, a translation or a remembered shape of GPL
code would taint brodo, and an agent reads whatever it is pointed at. A provenance claim
made after the fact cannot be checked.

## Decision

- An agent working in brodo reads only brodo, papers, public documentation and the public
  API of its dependencies; never the source of OpenVDB or OpenFOAM, nor the source or
  design of the copyleft projects its owner names (AGENTS.md S1).
- A Claude Code hook blocks a read, search, fetch or shell command whose path or URL names
  one of them. It matches names, not directories, so that no directory on the owner's
  machine is written into the repository.
- PROVENANCE.md has a row per source file, added with the file, naming the papers and
  documents it drew on and for what (S3); the audit checks that the row exists.
- Code copied from a permissively licensed crate keeps its notice and is recorded (S2).
- brodo holds no concept of its users; a missing feature a user needs is recorded as a
  request rather than worked around elsewhere (S4).

## Consequences

The hook is bypassed by a name written without a slash before it, and what was read leaves
no trace, so S1 rests on the agent. PLANNING.md §6 reads documentation rather than a
dependency's source. The licence is to be confirmed for each form of distribution before
any is made (ROADMAP.md §4).

## Amendment

- **Date**: 2026-10-16

S4 was limited to library code under `crates/*/src/`. P1's vertical slice checks the API
with examples written as its users would use it, a diffusion over several fields among
them; a ban on solver concepts in all code would have forbidden that. Examples, tests and
benchmarks may use such concepts, written from textbook material under S1.

## Amendment

- **Date**: 2026-10-20

The human decided that brodo's repository need not know its owner's other projects, and
asked that their names be taken out of the code, the rules and the records. S1 named two
of them, and so did the guard that enforces it. The human decided:

- The names move to a list outside the repository, one a line, at
  `$BRODO_FORBIDDEN_SOURCES` or `~/.config/brodo/forbidden-sources`. S1 forbids the
  source and design documents of the projects in it, besides OpenVDB's and OpenFOAM's,
  which stay named: they are other people's projects, named for a technical reason.
- `scripts/hooks/provenance_guard.py` blocks the names in the list as before, and the
  audit fails on a machine without the list, as it does without the personal rules.
- The records that named the projects describe them by kind: a CFD solver, a shape
  kernel, copyleft code. The history keeps the old text; rewriting it waits for a
  decision to make the repository public.

Shown to fire: with a name in a list given by `$BRODO_FORBIDDEN_SOURCES`, a path naming it
was blocked, and passed without it; both names in the owner's list were blocked, and
passed with no list; a path naming OpenVDB was blocked with no list; `--check` failed with
no list and with an empty one, and passed with the owner's.

## Amendment

- **Date**: 2026-10-24

The owner confirmed that OpenVDB is licensed under the Mozilla Public License 2.0, before
and since the Academy Software Foundation took it on. MPL-2.0 is copyleft file by file: a
modified file is distributed under it, while code that only links to OpenVDB need not be.
brodo does not link to OpenVDB but implements the same ideas itself, so a translation or
port of an OpenVDB file into brodo could be such a modification, and would bind that file
of brodo to MPL-2.0, which MIT OR Apache-2.0 cannot hold. S1 stands for OpenVDB on that
ground. MPL-2.0's patent grant covers users of OpenVDB's code, not an implementation drawn
from the papers, so it gives brodo none.

## Amendment: publication

- **Date**: 2026-10-25

Before the repository was made public, the owner weighed patents and the licence terms for
distributing the source on GitHub, on a preliminary search the agent made of Google
Patents by phrase and inventor (2026-10-25), not a freedom-to-operate study:

- The VDB tree (Museth 2013): of nine filings naming Museth, the one near brodo's design,
  US20100114909A1 (Digital Domain, filed 2008), claimed blocks of values with a bit mask of
  the non-zero ones; Google Patents listed it as abandoned, with no granted family member.
- Generalised and fast winding numbers (Jacobson et al. 2013, Barill et al. 2018): no
  filing named Barill; the one naming Jacobson with "winding number", Adobe's
  US11810255B2, concerned swept volumes. Filings found by the phrases used winding numbers
  as a part, none claimed computing them.
- The watertight segment–triangle test (Woop et al. 2013): NVIDIA's family "Watertight ray
  triangle intersection" (priority 2018, expected to expire 2038; US10825230B2,
  US11704863B2, US12067669B2 granted, US20240355039A1 pending) claimed hardware circuitry
  or a traversal unit in every independent claim read, and cited Woop et al. as prior art
  for software on a CPU; colino is software on a CPU.
- Licences: the source is distributed under MIT OR Apache-2.0, every dependency held to
  `deny.toml`'s allow-list by `cargo deny`, and no copyleft source read (S1), each source
  file's papers recorded in PROVENANCE.md.

The owner judged nothing found to stand in the way of publishing the source, and decided
that the question is asked again when the form of distribution changes, such as
registration on crates.io or an implementation in hardware, or when the pending
application is granted with claims beyond hardware.

