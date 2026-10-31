# ADR-0045: brodo reads and writes `.vdb` files itself, not through `vdb-rs`

- **Status**: Accepted
- **Date**: 2026-10-31

## Context

B-006 asked whether `vdb-rs` meets Rules 12 and 13 on every target and was written apart
from OpenVDB's source; ARCHITECTURE.md §4.7 had `.vdb` files read through it. ADR-0044 let
OpenVDB be read from `v12.0.0` on to learn the format, so that brodo could read and write
the files itself. Measured on 2026-10-31:

- `vdb-rs` 0.6.0, MIT, has no features, and every dependency of its is required; among
  them `blosc-src`, which with `lz4-sys` builds C through `cc` (`cargo tree -i cc` in an
  empty crate depending on it), against Rule 12.
- Its documentation reads files only: writing, older versions, multi-pass point data and
  delayed loading are listed as missing; it states that it follows "the original OpenVDB
  implementation".
- Its seven versions were published between 2023-03-27 and 2024-01-04 (the crates.io
  API), all before OpenVDB's relicensing to Apache-2.0 on 2024-10-31; whether it holds
  translated MPL-2.0 code cannot be checked, since S1 lets brodo read a dependency's API and
  documentation only.
- brodo's tree has OpenVDB's default shape: nodes of 32³, 16³ and 8³ places under a root
  (docs/formats.md §1.2).
- Homebrew carries OpenVDB 14.0.0, its formula's licence still given as MPL-2.0; it builds
  C++, outside brodo's build.

## Decision

The owner decided:

- `vdb-rs` is not used; B-006 is closed by this record. brodo reads and writes `.vdb`
  itself, in Rust, learning the format from OpenVDB at the tag `v14.0.0`
  (`ecae2bda85744412b0cd47e1b8e1e4e76beb7b88`) under ADR-0044, each file read recorded in
  PROVENANCE.md, the format written in docs/formats.md.
- Writing is part of P7, so that brodo's grids open in the tools that read `.vdb`.
- In steps, each a spike and an ADR before it is built: writing first, of float and
  double grids on brodo's tree, with what metadata, transform and compression the spike
  finds needed; then reading, of the files brodo writes and those OpenVDB 12 to 14 writes
  for the same grid types by default. Older formats, point data, frustum transforms and
  delayed loading are left out, recorded as requests if a user asks.
- What a written file holds is checked by a reader apart from brodo's: OpenVDB's own
  tools, installed by the owner, outside the build and the gate.

## Consequences

brodo carries the cost of the format: versions, per-node compression, codecs and
metadata, each added only as a step's spike shows it needed; a dependency for a codec
brodo lacks, such as zlib's, is approved before it is added (CODING.md Rule 20). Rule 12
holds with no exception. The check against OpenVDB's tools is held by the spike and by
hand, not by the gate, which builds no C++.
