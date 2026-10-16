# ADR-0012: brodo is a standalone sparse-volume base, in two crates

- **Status**: Accepted
- **Date**: 2026-10-16

## Context

the solver (a CFD solver) and the design platform's shape kernel `the shape kernel` both need sparse volumes held,
edited and stored efficiently in pure Rust. Their needs differ: shared topology, neighbour
tables and determinism for one; tiles, random access and dilation for the other.
Converting outside meshes to signed distance fields needs the same tree and its topology
analysis.

## Decision

- brodo is designed as a part published on its own, knowing nothing of its users.
- Crate `brodo` holds the tree, fields, the frozen view, the file formats and the generic
  topology analysis; crate `colino` converts outside shapes to narrow-band SDFs on it and
  reports topology changes.
- Out of scope: level-set operations and SDF-to-mesh (`the shape kernel`'s, until shown to be
  general), solvers and discretisation (the solver's), visualisation.
- The kitchen names are for the repository and crates only; types and functions have
  plain English names.

## Consequences

A request from a user that falls outside this scope is recorded, not built (AGENTS.md S4).
