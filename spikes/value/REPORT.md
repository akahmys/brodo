# Spike: value (B-017)

Measured on 2026-10-26 with rustc 1.98.1, against brodo at `803a161`, before `Value` was
split; it does not build against the split trait. Reproduce from the repository's root at
that commit:

```bash
cargo run --release --manifest-path spikes/value/Cargo.toml -- out/value
```

## Question

**B-017**: is `Value` sealed, so that only brodo assigns type codes? A code names a grid's
value type in its frozen header and a cache file's directory (docs/formats.md §1.1); a
reader knows codes 1 to 6 and refuses others.

## Method

Two types outside brodo implement `Value`: `Material`, two `u16`, with code 99, which brodo
does not assign; and `Count`, an `i32`, with code 1, `f32`'s. Each is put in a field and
updated in parallel, frozen and viewed, and written to a cache file and read back
(`out/value/results.txt`).

## Results

| Step | `Material`, code 99 | `Count`, code 1 |
| :--- | :--- | :--- |
| A field in memory, updated in parallel | works | works |
| `freeze`, then `GridView` of the same type | works | works |
| A cache file written | succeeds | succeeds |
| That file read back by the same build | refused: `WrongValueType { found: 99 }` | read |
| Its grid viewed as `f32` | — | read: 1 065 353 216 is 1.0 |

## Findings

1. A type of a user's own serves in memory, and in a frozen buffer the same program views.
2. Its cache file is written without complaint and read by no build, the one that wrote
   it included: the reader checks each grid with the types it knows.
3. A user's code that is brodo's, at the same size, passes every check and is read as
   brodo's type; so would any code brodo assigns later that a user already took.

## What this leaves to decide (for the ADR)

- Whether `Value` stays open, is sealed, is split into an open trait for memory and a
  sealed one for files, or keeps a range of codes for users.
