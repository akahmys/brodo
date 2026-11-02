# ADR-0012: brodo is a standalone sparse-volume base, in two crates

- **Status**: Amended
- **Date**: 2026-10-16

## Context

A CFD solver and a shape kernel both need sparse volumes held,
edited and stored efficiently in pure Rust. Their needs differ: shared topology, neighbour
tables and determinism for one; tiles, random access and dilation for the other.
Converting outside meshes to signed distance fields needs the same tree and its topology
analysis.

## Decision

- brodo is designed as a part published on its own, knowing nothing of its users.
- Crate `brodo` holds the tree, fields, the frozen view, the file formats and the generic
  topology analysis; crate `colino` converts outside shapes to narrow-band SDFs on it and
  reports topology changes.
- Out of scope: level-set operations and SDF-to-mesh (the shape kernel's, until shown to
  be general), solvers and discretisation (the solver's), visualisation.
- The kitchen names are for the repository and crates only; types and functions have
  plain English names.

## Consequences

A request from a user that falls outside this scope is recorded, not built (AGENTS.md S4).

## Amendment

- **Date**: 2026-10-20

A user asked for two-dimensional runs, and later axisymmetric ones (ROADMAP.md §4, R4).
They run today as a grid one leaf thick, each value of a field one voxel thick in z held
in a leaf of eight layers. A mode of its own would double the voxel coordinate, the tree,
the frozen layout and the files. The human decided:

- A grid one leaf thick is the supported way to keep two-dimensional fields, and is not
  expected to change; brodo has no two-dimensional mode.
- Axisymmetry is a discretisation, outside brodo's scope (ARCHITECTURE.md §1).

## Amendment

- **Date**: 2026-11-02

Two open questions on scope were closed by this decision, with no spike, since they asked
what brodo is rather than what a measurement could show (ROADMAP.md §3). The human
decided:

- B-003: level-set operations, CSG and redistancing, do not belong in brodo. One shown to
  be general is a request (ROADMAP.md §4), taken up as any other.
- B-004: no IR schema shared by brodo's users lives in this repository. Its content, a
  mesh's attributes, conditions and settings, is its users' concepts, which brodo does not
  know (AGENTS.md S4), and holding it here would tie their changes to brodo's releases.
  brodo's promise to them is its behaviour, its bytes and its API (AGENTS.md); what they
  agree on above it is theirs.
