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

## Amendment

- **Date**: 2026-10-17 (P1's Miri run)

T7 was run before P1 closed, with Miri 0.1.0 (nightly of 2026-07-21): the module's two
tests, in `tests/cache_format.rs`, were ignored under Miri as decided above, so the run
executed none of the module's unsafe code. Beyond T7, with isolation disabled, the rest of
`cache_format` passed under Miri (six tests, 37 minutes). In `frozen_format`, Miri's
Stacked Borrows model reported undefined behaviour in `crossbeam-epoch` 0.9.21
(`internal.rs:567`), reached through `rayon-core` 1.13.0's work stealing when the test ran
`freeze` in a pool of eight threads; no brodo code was on the stack. A run under Tree
Borrows, to tell a fault of the model from one of the crate, was stopped unfinished after
an hour, by the human's choice.

## Amendment

- **Date**: 2026-10-17 (the mapping kept)

Since T7 runs none of the module's code under Miri, the human weighed removing the mapping,
which would leave brodo with no unsafe code. Measured on an Apple M5 with a 5.4 MB cache
file, reading it by copy took about 249 µs and mapping it about 15 µs; the mapping checks
the file's structure and leaves the values unread until used. Estimated from the bytes per
leaf (about 2.1 KB for one `f32` field) and the leaves of narrow-band spheres (about
0.41 r²), a rocket engine of LEAP 71's published scale at print resolution (0.3 to 0.5 mm),
or a 3000 mm mixed-flow pump with its near-wall region at 0.3 mm, reaches tens to hundreds
of GB across its fields, beyond a workstation's memory, where a copy cannot open the file
and a mapping reads only what is used. These are estimates on assumed surface areas and
volumes, not measurements. The human decided to keep the mapping.

## Amendment

- **Date**: 2026-10-17 (T7's scope)

T7 as written ran none of the mapping module's code, and the existing tests were too slow
under Miri to stand in: with eight cases each, `cache_format` took 2 min 44 s under
Stacked Borrows, `parallel::tests` 39 minutes under Tree Borrows, and `frozen_format`
had not finished its first test after 82 minutes, the threaded tests being the slow ones.
Miri's purpose being undefined behaviour, not coverage, T7 was redefined: three tests
named `miri_*`, in `tests/miri_paths.rs`, take each path through unsafe code in brodo's
dependencies on a grid of three leaves and two tiles, reading a frozen grid in place and a
cache file's bytes (`zerocopy`, under Stacked Borrows) and running the leaf-parallel
update and reduction on four threads (`rayon`, under Tree Borrows, since Stacked Borrows
reports `crossbeam-epoch` (ADR-0015), and with the leak check off, since rayon's threads
outlive the test). They ran in 56 s and 25 s with no report; three threads took part in
the update, natively and under Miri. Each checks every value against the field it came
from, so it also fails natively: reading a leaf's values with x and y swapped, and
handing the update its leaves in reverse, each failed it.


## Amendment

- **Date**: 2026-10-20 (T7's scope)

colino brought dependencies of its own. `rawzip`, `miniz_oxide` and `quick-xml` each
forbid unsafe code at their roots, but `quick-xml` scans XML with `memchr`, much of which
is unsafe. T7 named brodo's dependencies only. The human decided:

- T7 covers the dependencies of every crate in the workspace. colino's
  `tests/miri_paths.rs` reads a 3MF package of one tetrahedron, written by `rawzip`'s own
  writer, under Stacked Borrows; it checks every coordinate's bits and every triangle.

It ran under Miri in 2 s with no report. Natively, the triangles' orientation flipped in
the reader failed it.

## Amendment

- **Date**: 2026-10-25

ADR-0036 made `CacheFile::map` an `unsafe fn`, its promise, that the file is not changed
while mapped, stated under `# Safety` where the compiler asks the caller for it. The
function moved into the mapping module, the one file allowed `unsafe`. Its callers then
needed `unsafe` too: brodo's own test of mapping, `cache_format.rs`, and its benchmark.
The human decided that the files `UNSAFE_CALLERS` lists in `project.py` may hold `unsafe`
only as `unsafe { CacheFile::map(…) }`, under a `// SAFETY:` comment and the file's
`#![expect(unsafe_code, …)]`, and `scripts/audit/source.py` holds them to that form; a
user outside the workspace writes its own `unsafe`, as for any `unsafe fn`. Shown to
fire: a call of `map` in another test file failed Rules 4 and 19; another `unsafe` in a
listed file failed Rule 4; the listed file without its `#![expect]` failed rustc's
`unsafe_code`.

