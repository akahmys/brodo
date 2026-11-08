# ADR-0008: Unsafe code is confined to one module, brodo's memory mapping, and run under Miri

- **Status**: Amended
- **Date**: 2026-10-16

Condensed on 2026-11-08 (ADR-0001): each decision is kept with its reason and what it set
aside; the text before is in git, this file as of `53f69d5`.

## Context

The template forbade `unsafe` outright. The cache profile is read without a copy by
mapping a file into memory, and mapping a file is an unsafe call in Rust, since another
process can change the file under the mapping. Zero-copy reading of the mapped bytes
itself is done by `zerocopy`, which needs no unsafe code in brodo.

## Decision

The human decided, on 2026-10-16 unless dated otherwise.

- **One module.** The workspace lint `unsafe_code` is `deny`, not `forbid`, so that one
  file can carry `#![expect(unsafe_code, reason = "…")]`: the single entry of
  `UNSAFE_MODULES` in `scripts/audit/project.py`, `crates/brodo/src/mapping.rs`, whose one
  unsafe operation is `memmap2::Mmap::map` (2026-10-17). `scripts/audit/source.py` fails
  the word `unsafe` in code anywhere else and more than one entry in the list. Each unsafe
  block holds one operation under a `// SAFETY:` comment (clippy
  `undocumented_unsafe_blocks`, `multiple_unsafe_ops_per_block`).
- **Its callers** (2026-10-25). `CacheFile::map` is an `unsafe fn` in the module, its
  promise, that the file is not changed while mapped, stated under `# Safety` where the
  compiler asks the caller for it (ADR-0036). The files `UNSAFE_CALLERS` lists, brodo's
  test of mapping and its benchmark, may hold `unsafe` only as
  `unsafe { CacheFile::map(…) }`, under a `// SAFETY:` comment and the file's
  `#![expect(unsafe_code, …)]`. A user outside the workspace writes its own `unsafe`.
- **A changed file is the caller's** (2026-10-17). That another process changes or
  truncates a mapped file is accepted rather than guarded, since a lock would not hold
  across platforms; the mapped bytes are still checked whole as untrusted input.
- **The mapping kept** (2026-10-17). Removing it would leave brodo with no unsafe code. On
  an Apple M5 a 5.4 MB cache file took about 249 µs to read by copy and 15 µs to map, the
  mapping leaving values unread until used; grids estimated, not measured, at tens to
  hundreds of GB across their fields would not open by copy on a workstation at all.
- **Miri runs the paths through unsafe code** (TESTING.md T7), on nightly before a
  milestone closes, outside the gate, as the milestone's checklist chooses (ADR-0033).
  Miri cannot map a file ("unsupported operation: Miri does not support file-backed memory
  mappings"), so the mapping's tests are ignored under it and the module's soundness rests
  on its `SAFETY` argument. Miri's purpose being undefined behaviour, not coverage, T7
  runs tests named `miri_*` taking each path through unsafe code in the workspace's
  dependencies on a few leaves (2026-10-17, 2026-10-20): a frozen grid and a cache file's
  bytes read in place (`zerocopy`, Stacked Borrows), the leaf-parallel update and
  reduction on four threads (`rayon`, Tree Borrows, leak check off since its threads
  outlive the test), and colino's 3MF reader (`quick-xml`'s `memchr`, Stacked Borrows).
  Each checks every value, so that it fails natively too. The existing tests were too
  slow to stand in: `frozen_format` had not finished one test under Miri in 82 minutes.

## Consequences

A second unsafe site needs this record amended. Under Stacked Borrows, Miri reports
undefined behaviour in `crossbeam-epoch`, reached through rayon's work stealing with no
brodo code on the stack (ADR-0015); the rayon paths are run under Tree Borrows.
