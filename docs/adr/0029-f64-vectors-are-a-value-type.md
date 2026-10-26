# ADR-0029: `[f64; 3]` is a value type

- **Status**: Amended
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

## Amendment: a value type for files is brodo's

- **Date**: 2026-10-26

B-017 asked whether `Value` is sealed. `spikes/value/` gave two types of a user's own a
field, a frozen view and a cache file (its REPORT.md): both served in memory and in a view
of their own frozen bytes; a cache file of the type with code 99 was written without
complaint and refused by every build, the one that wrote it included; and a user's `i32`
with `f32`'s code 1 was read as `f32`, 1 065 353 216 as 1.0. Sealing `Value` whole would
forbid types used in memory only. The human decided:

- `Value` stays open: a field, its parallel update, gathers, accessors and analyses take
  any type that implements it. It no longer carries a type code.
- `Stored: Value` is sealed and carries `TYPE_CODE`; brodo implements it for its six
  types alone. `freeze`, `GridView`, `CacheEntry::new` and `CacheFile::view` take a
  `Stored` type, so that a file holds only codes brodo assigns and reads.
- A user implementing `Value` drops `TYPE_CODE` from its implementation; CHANGELOG.md lists
  the change as breaking.
