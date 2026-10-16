# ADR-0009: Indexing is banned, and fuzzing required, per parsing module rather than per crate

- **Status**: Accepted
- **Date**: 2026-10-16

## Context

The template banned indexing and slicing in every crate that reads untrusted input. Both
brodo (grid files, `.vdb`) and colino (STL, OBJ, 3MF) read files they did not write, so
the ban would have covered brodo's whole voxel core, where leaf arrays are indexed by
offsets the tree has already validated.

## Decision

- `UNTRUSTED_INPUT` lists modules, as paths under `crates/<crate>/src/`, that parse
  untrusted bytes. The ban (CODING.md Rule 3) holds in them only: the audit filters
  clippy's `indexing_slicing` findings to those paths.
- Such a module is listed by the commit that creates it, and its crate is fuzzed, one
  target per way in (TESTING.md T6).
- A listed path that does not exist fails the audit.

## Consequences

Data that leaves a parsing module is validated there, so code elsewhere can index by it.
That a new parser is listed at all is held by review, not by a check.
