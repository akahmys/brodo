# ADR-0037: The file formats are specified in one document apart from the code

- **Status**: Amended
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

## Amendment

- **Date**: 2026-10-25

The document was written as decided, with the narrow-band SDF convention (ADR-0039), the
media type names (ADR-0041) and the version policy (ADR-0038). The reader,
`scripts/formats/read.py`, runs under `crates/brodo/tests/format_document.rs` in the gate's
tests: on the four golden files it must print every grid's metadata, background, leaves,
tiles, some voxels found by the document's lookup and each Hermite set's verdict as the
crates read them; on 2942 damaged copies, four values in each of the words the test picks,
it must accept and refuse as the crates do. Shown to fire: without the check of the
buffer's length in blocks, or of the cache header's unused bytes, or of an edge whose
signs agree holding no crossing, the damaged copies disagreed; with a leaf's voxels
ordered y fastest, the golden files did. Before the voxels were printed, that last break
passed, the leaves' bytes being compared whole.

Writing it down found what a reader of version 1 does not check, now stated in the
document: a frozen buffer's padding and the background's bytes beyond its size, the order
of its nodes and whether every node and leaf is reached, and a cache file's bounds against
its grid. Whether to check them is left to a later format version.

## Amendment: where the reader lives, and references to the document

- **Date**: 2026-10-25

A review found two gaps the first commit left. The reader was put in `scripts/formats/`,
where CODING.md Rule L4 allows no script; the owner had it moved to
`scripts/audit/formats.py`, among the checks, since the gate's tests run it. And
`scripts/audit/documents.py` checked `NAME.md §N` references to root documents only, so
the references to `docs/formats.md §N` went unchecked; it now reads the headings of
`docs/*.md` too, and takes a name in backquotes as well as in a link or bare. Shown to
fire: a reference to a section the document lacks failed the check with the name in
backquotes, after a link, and bare.


## Amendment: OpenVDB files

- **Date**: 2026-11-01

docs/formats.md §9 gained a test vector when brodo began writing `.vdb` files (ADR-0046):
`crates/brodo/tests/data/vdb_v225.bin`, a float level set, a double grid and an empty
grid. The owner had the reader cover it, as recommended: `scripts/audit/formats.py` reads
an OpenVDB file from §9 alone, and `crates/brodo/tests/vdb_format.rs` holds every grid's
metadata, transform, background, leaves and tiles it prints to the grids brodo wrote.
brodo does not read `.vdb` yet, so no damaged copy is compared; that waits for the reading
step. Shown to fire: with a leaf's values taken y fastest by the reader, the test failed.
OpenVDB 14.0.0 read the same file back with every voxel and tile the same, and wrote it
again the same but for `file_mem_bytes`, by hand with `spikes/vdb/`'s oracle.

## Amendment: the hierarchy of truth names the exception

- **Date**: 2026-11-02

The review of the whole repository found AGENTS.md's "Measurement outranks documentation"
holding that a document disagreeing with a measurement is corrected, with no word of this
record's rule that code disagreeing with docs/formats.md is corrected instead. The owner
decided, on 2026-11-02, that AGENTS.md names the exception.

## Amendment: a region read

- **Date**: 2026-11-02

An outside review found that the reader read §8.6, an archive's region, not at all, so that
the section, and the rule 6 added to it on 2026-11-02, held only by the crates' tests. The
owner decided, on 2026-11-02, that it be fixed: the reader reads a grid's region through a
box as §8.6 says, with `--region`, and `format_document.rs` compares it with the crates' on
each archive golden file through six boxes, by every voxel and leaf, accepts and refuses as
they do 3 × 2 sets of damaged copies, some a whole read refuses and a region answers, and
refuses with them a region walking to a leaf two lower nodes share, in an archive made so,
while a region reaching it once is read. Shown to fire: without §3.4's check of the
metadata, with a box met one voxel too far on either side, with a quantised leaf's values
left unrestored, and without the check of a leaf reached twice, the reader disagreed.

## Amendment: a check added to the reader as it is written

- **Date**: 2026-11-04

P8's retrospective found that docs/formats.md §2.7 stated version 2's rule 9, root tiles
numbered in table order, wrongly, and that nothing found it until the code was written,
since the reader gained the rule only with the code. The owner decided, on 2026-11-04,
that a check the document gains for a reader is added to `formats.py` in the same commit
(TESTING.md T13), so that the gate holds the golden files already there to it at once;
that it is added in the same commit nothing checks, since a commit-msg hook asking for the
reader with every change to the document would ask it of every change of wording too.
