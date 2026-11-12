# ADR-0037: The file formats are specified in one document apart from the code

- **Status**: Amended
- **Date**: 2026-10-25

Condensed on 2026-11-08 (ADR-0001): each decision is kept with its reason and what it set
aside; the text before is in git, this file as of `53f69d5`.

## Context

brodo's files were described in ARCHITECTURE.md, in ADR-0016, ADR-0029 and ADR-0032, and in
the comments of `frozen.rs`, `cache.rs` and `hermite.rs`; the bytes themselves were fixed
by golden files in `crates/brodo/tests/data/`. A reader in another language, or one that
will not depend on the crates, had no single place to learn them from.

## Decision

The owner chose, on 2026-10-25 unless dated otherwise, the recommended option of each
point.

- **One document**, `docs/formats.md`, not one per format: the cache profile holds the
  frozen layout and Hermite data lives in it, so one reader reads them together. It covers
  every file brodo writes, the narrow-band SDF's conventions (ADR-0039), the media types
  (ADR-0041) and the version policy (ADR-0038). ARCHITECTURE.md and the ADRs point to it for
  the bytes and keep only the design.
- **Every byte stated**: offsets, sizes, endianness, alignment, padding, each field's
  meaning and the checks a reader must make, the golden files named as its test vectors;
  a description with the code as the authority would not let another language read and
  write the files.
- **The document wins.** Where it and the code differ, the code is corrected, and a
  difference found is a defect of the format's version; AGENTS.md's hierarchy of truth
  names this exception (2026-11-02).
- **A reader holds it true**, not review alone, which would not notice what the document
  leaves out. `scripts/audit/formats.py`, written from the document alone in Python and
  placed among the checks (CODING.md L4), runs under
  `crates/brodo/tests/format_document.rs`: on every golden file it must print what the
  crates read, grids, metadata, leaves, tiles, voxels found by the document's lookup,
  Hermite verdicts, `.vdb` grids (2026-11-01) and an archive's regions through boxes
  (2026-11-02); on thousands of damaged copies it must accept and refuse as the crates do.
  A check the document gains for a reader is added to `formats.py` in the same commit
  (TESTING.md T13, 2026-11-04): version 2's rule 9 had been stated wrongly and found only
  once the code was written. That it is the same commit nothing checks, since a hook asking
  it of every change to the document would ask it of every change of wording.
- `scripts/audit/documents.py` resolves `docs/formats.md §N` references as it does a root
  document's, the name in a link, backquotes or bare (2026-10-25).

## Consequences

What a reader does not check is stated in the document rather than left unsaid; writing
it down first found the gaps of version 1 that version 2 closed (ADR-0052).

## Amendment

- **Date**: 2026-11-09

Issue 15 reported that `formats.py`'s `hermite`, like brodo's own `check_version`, took an
entry's `hermite.set` for an earlier version's set though the entry held
`brodo.hermite.set`, against docs/formats.md §4.2, rule 1, and that `formats.py`'s `read`,
`format_document.rs`'s own reader of the crates and the Hermite fuzz target named the sets
of a file from its grids alone, so that a set named only by an entry of bytes was never
checked. The two readers agreed, each wrong, so that comparing them found nothing. The
owner decided, on 2026-11-09, as proposed, that `formats.py`, the crates' reader and the
other two readers be fixed together, in one commit with this amendment. So that agreeing
is not enough, `format_document.rs` holds both readers to a verdict the test states: on a
file whose set carries a free `hermite.set` naming another set of this version, and on one
whose set is named by its crossings alone. Undoing either of `formats.py`'s fixes fails
that test (T3).

## Amendment: every reader held to a stated verdict

- **Date**: 2026-11-12

P12's retrospective found the case of 2026-11-09 above general: a check that compares two
readers finds nothing when both misread the format alike. The owner decided, on 2026-11-12,
that TESTING.md T13 say that `formats.py` and the crates' readers are each held to a
verdict a test states, not only to one another. Held by review: no script can tell a stated
verdict from a compared one.
