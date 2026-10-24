# ADR-0037: The file formats are specified in one document apart from the code

- **Status**: Accepted
- **Date**: 2026-10-25

## Context

brodo's files were described in ARCHITECTURE.md §4.6, §4.7 and §4.9, in ADR-0016,
ADR-0029 and ADR-0032, and in the comments of `frozen.rs`, `cache.rs` and `hermite.rs`;
the bytes themselves were fixed by golden files in `crates/brodo/tests/data/`. Some users
pass files between processes, and a reader in another language, or one that will not
depend on the crates, had no single place to learn them from.

## Decision

The owner chose, on 2026-10-25, the recommended option of each point:

- **a. Where.** (1) One document, `docs/formats.md`, covering the frozen layout (version 1),
  the cache profile (format version 1), Hermite data (ADR-0032) and, if ADR-0039 makes them
  public, the narrow-band SDF's conventions. (2) One document per format in `docs/formats/`.
  *Recommended: (1)*: the cache profile holds the frozen layout and Hermite data lives in
  it, so one reader reads all three.
- **b. How exact.** (1) Every byte stated: offsets, sizes, endianness, alignment, padding,
  each field's meaning and the checks a reader must make, with the golden files named as
  its test vectors. (2) A description, with the code as the authority. *Recommended: (1)*,
  which is what lets another language read and write the files.
- **c. Which wins when they differ.** (1) The document; the code is corrected, and a
  difference found is a defect of the format's version. (2) The code. *Recommended: (1)*.
- **d. What holds the document true.** (1) A small reader written from the document alone,
  in Python under `scripts/`, run by the gate on the golden files, comparing every value
  with what the crates read. (2) Review alone. *Recommended: (1)*: it fails when the
  document leaves out what a reader needs, which review would not notice.
- ARCHITECTURE.md §4.6, §4.7 and §4.9 and the ADRs then point to the document for the bytes
  and keep only the design.

## Consequences

The document is written in P4 and checked as AGENTS.md's writing rules check the others.
