# ADR-0007: Copyleft and sibling sources are never read, and every source file records what it drew on

- **Status**: Amended
- **Date**: 2026-10-16

## Context

brodo is permissively licensed and is the base of a project group in which the solver is
GPLv3 and builds on OpenFOAM's ideas. A port, a translation or a remembered shape of GPL
code would taint brodo, and an agent reads whatever it is pointed at. A provenance claim
made after the fact cannot be checked.

## Decision

- An agent working in brodo reads only brodo, papers, public documentation and the public
  API of its dependencies; never the source of the solver, OpenVDB or OpenFOAM, nor the solver's or
  the design platform's design or code (AGENTS.md S1).
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
