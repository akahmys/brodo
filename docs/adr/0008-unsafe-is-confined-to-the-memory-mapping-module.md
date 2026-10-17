# ADR-0008: Unsafe code is confined to one module, brodo's memory mapping, and run under Miri

- **Status**: Amended
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

## Amendment

- **Date**: 2026-10-17

The module was created as `crates/brodo/src/mapping.rs`, listed in `UNSAFE_MODULES`; its
one unsafe operation is `memmap2::Mmap::map`. Before it was written the human decided two
points. Miri cannot map a real file, and the module has no other unsafe code to reach
through a buffer, so its mapping tests are ignored under Miri with that reason; T7 still
runs, and the module's soundness rests on its `SAFETY` argument, not on Miri. And the risk
that another process changes or truncates a mapped file was accepted rather than guarded,
since a lock would not hold across platforms: `CacheFile::map` states that the caller keeps
the file unchanged while it is mapped, and the mapped bytes are still checked whole as
untrusted input.

Measured the same day with Miri 0.1.0 (nightly of 2026-07-21), isolation disabled: the
mapping tests, run with their `ignore` removed, stopped in `memmap2`'s call to `mmap` with
"unsupported operation: Miri does not support file-backed memory mappings".
