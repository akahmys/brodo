# ADR-0062: What rounds off 1.0 is added

- **Status**: Accepted
- **Date**: 2026-11-08

## Context

TESTING.md T14's survey before 1.0 (2026-11-05) left gaps a later MINOR adds without
breaking a user. The owner split them on 2026-11-08: those that round off what is there
are P10's (ROADMAP.md §2), released as 1.1.0; those that add a capability are GitHub
issues. Of brodo's core, a field's background could be read but not set; a `LeafNumber`,
a `TileNumber` and an `Offset` gave their number, and an `Offset` its step, but none was
made from them; and a field went into an `AnyField` but came out only by a `match`. Of
its formats, an archive's grid read back held no trace of how its values were encoded.

## Decision

The owner decided, on 2026-11-08, as proposed:

- `Field::set_background(value)` sets the value outside the tree only; the leaves' and
  tiles' values are unchanged, those equal to the old background among them, so that
  setting it is predictable and costs nothing.
- `LeafNumber::new(n)` and `TileNumber::new(n)` cannot fail: a number its topology did not
  give is refused where it is used, as `UnknownLeaf` or `UnknownTile`, as it is now.
- `Offset::from_index(i)` and `Offset::from_delta(d)` are `index`'s and `delta`'s
  counterparts, giving none for a number from 26 on, the step of none, or a step beyond
  one leaf.
- `Field<T>` implements `TryFrom<AnyField>`, its error the `AnyField` given back unchanged,
  so that a caller trying one type after another loses nothing.
- Of brodo's formats, an archive's grid read whole, as a `GridEntry`, or as a `Region`
  gives the `Encoding` its values were written with, which reading it otherwise loses; a
  cache file's grid gives `Exact`. The method exists with the `archive` feature, as
  `Encoding` does.

## Consequences

Each addition is held by a unit test that fails when it is broken. Nothing a user wrote
breaks, and the bytes are unchanged.
