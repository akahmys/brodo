# ADR-0008: Unsafe code is confined to one module, brodo's memory mapping, and run under Miri

- **Status**: Accepted
- **Date**: 2026-10-16

## Context

The template forbade `unsafe` outright. The cache profile is read without a copy by
mapping a file into memory, and mapping a file is an unsafe call in Rust, since another
process can change the file under the mapping. Zero-copy reading of the mapped bytes
itself is done by `zerocopy`, which needs no unsafe code in brodo.

## Decision

- The workspace lint `unsafe_code` went from `forbid` to `deny`, so that one file can
  carry `#![expect(unsafe_code, reason = "…")]`. That file is the single entry of
  `UNSAFE_MODULES` in `scripts/audit/project.py`: brodo's memory-mapping module, listed
  when it is created.
- `scripts/audit/source.py` fails the word `unsafe` in code anywhere else, tests included,
  and more than one entry in the list.
- Each unsafe block holds one operation under a `// SAFETY:` comment (clippy
  `undocumented_unsafe_blocks`, `multiple_unsafe_ops_per_block`).
- The module's tests run under Miri on nightly before every milestone closes, outside
  the gate as the fuzzer is (TESTING.md T7).

## Consequences

A second unsafe site needs this record amended. Until the module exists the list is empty
and the rule is as strict as before.
