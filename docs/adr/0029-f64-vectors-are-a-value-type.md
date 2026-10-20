# ADR-0029: `[f64; 3]` is a value type

- **Status**: Accepted
- **Date**: 2026-10-20

## Context

A user keeping time-stepped fields on brodo's tree asked for vector fields in double
precision (ROADMAP.md §4, R2). The value types were `f32`, `f64`, `u8`, `u32` and
`[f32; 3]` (ARCHITECTURE.md §4.8). A value type names itself in a frozen grid's header and
a cache file's directory by `Value::TYPE_CODE`, 1 to 5; a code chosen outside brodo makes
files no other build reads. A build that does not know a code refuses the grid, naming it
(`FormatError::WrongValueType`).

## Decision

The human decided, on 2026-10-20:

- `[f64; 3]` implements `Value`, `TYPE_CODE` 6, compared bit by bit per component, as
  `[f32; 3]` is.
- The frozen layout stays at version 1 and the cache format at version 1. The value
  sections hold 24-byte values within sections aligned to 32 bytes. The frozen header
  held the background in three words, too few for 24 bytes; it takes six, the three after
  them having been padding of zeros, which no reader checked and every older type leaves
  zero, so that a grid of any older type keeps its bytes (the committed files show it).
  TESTING.md V6's "any change to the bytes" does not cover this, since no file an older
  build writes changes and an older build refuses code 6 by name. CHANGELOG.md records the
  code and the first version that reads it.
- It is not `Ordered`, morphology's values being scalars (ADR-0025), and not a VTK type
  until vector output is decided (R7).
- `Value` stays open to types outside brodo; whether to seal it, so that only brodo assigns
  codes, is open (ROADMAP.md §3, B-017).

## Consequences

The Format tier keeps a committed `[f64; 3]` grid's bytes, beside the `f32` one. A cache
file holding a `[f64; 3]` grid is refused by brodo 0.2.0 and read from the next version.
