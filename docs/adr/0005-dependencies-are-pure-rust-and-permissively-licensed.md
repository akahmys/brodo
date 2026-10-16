# ADR-0005: Dependencies are pure Rust, permissively licensed, and declared once

- **Status**: Accepted
- **Date**: 2026-10-16

## Context

A dependency that compiles C brings a second toolchain and its failure modes into every
build. A copyleft dependency sets terms on the project's own licence before the project
has chosen one. Dependencies declared per crate drift apart in version and features.

## Decision

- No dependency compiles C or C++, checked by `cc` in `cargo tree` for each target listed
  in `scripts/audit/project.py`.
- `deny.toml` allows permissive licences only. A copyleft licence (GPL, LGPL, AGPL, MPL,
  EPL and the like) is added only by amending this record.
- Every dependency is declared once in `[workspace.dependencies]`, and a new external
  dependency is approved by the human before it is added.

## Consequences

A native library (MPI, a GPU runtime, a system library) needs an exception, by amending
this record and naming the crate.
