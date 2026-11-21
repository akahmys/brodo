# Roadmap

Where the work is, what is next, and what is still open. Rules are in the phase documents;
the design is in [ARCHITECTURE.md](ARCHITECTURE.md); versions are in
[CHANGELOG.md](CHANGELOG.md).

## 1. Status

Overwritten at the end of every unit of work (PLANNING.md §1). History is in git.

- **Done**: P14's issues 18 and 19. colino's `convert` finds signs only for the leaves
  kept in the band, the bytes written unchanged (77fdbc1). brodo's nodes find a child's
  offset from a table of counts before each mask word (ADR-0013's amendment of
  2026-11-21): random access in less than half its time and activation in 37 % less.
  Before it, P13 released as 1.4.0 (2026-11-18), an archive's region and listing read from
  only the bytes they need, labels a leaf at a time, conversions rebuilt from grids read
  back, one reader for any mesh format, and mestolo's encodings and slice options, without
  breaking a user: brodo, colino and mestolo published by an agent at the owner's word for
  this release too (ADR-0040 leaves it to the owner), the new API used from a new crate
  depending on them by version, `cargo install mestolo` installing the command, and issues
  5, 9, 10, 11, 12, 16 and 17 closed. Before it, 1.3.1 (2026-11-18): an archive's read,
  quadratic in its entries, fixed from v1.3.0 as SECURITY.md states, advisory
  GHSA-rh8f-f6hf-2c95 published and its CVE requested.
- **Next**: P14's next item (§2), issue 20.
- **Unresolved**: none. `leaf_parallel_update_x32`'s drift between sessions is measured:
  other processes' work on the performance cores moves it either way, and the item and its
  baseline are kept (B-026, ADR-0019's amendment of 2026-11-12).

## 2. Milestones

Named after the project group's phases (ADR-0011); the checks a milestone closes with are
TESTING.md §2's and its checklist lists them (ADR-0033). A closed milestone leaves this
file: what it brought is in its version's CHANGELOG.md section, and its checklist in git,
in this file as of `53f69d5` (P0 to P9), `711cf5b` (P10), `e27fd49` (P11), `6f1140a`
(P12) and `f0a9c44` (P13).

| Milestone | Version | Closed |
| :--- | :--- | :--- |
| P0, the repository with its rules | 0.1.0 | 2026-10-16 |
| P1, `brodo` | 0.2.0 | 2026-10-17 |
| P2, `colino` | 0.3.0 | 2026-10-22 |
| P3, R11: triangle groups and Hermite data | 0.4.0 | 2026-10-25 |
| P4, the public API, before it grows | 0.5.0 | 2026-10-25 |
| P5, colino: a sign without orientation, thin places, costs | 0.6.0 | 2026-10-28 |
| P6, brodo: archives and quantisation | 0.7.0 | 2026-10-31 |
| P7, brodo: OpenVDB files | 0.8.0 | 2026-11-02 |
| The reviews after P7, no milestone | 0.8.1, 0.8.2 | 2026-11-02 |
| P8, looking inside, and the formats settled | 0.9.0 | 2026-11-04 |
| P9, 1.0 on crates.io | 1.0.0 | 2026-11-08 |
| P10, rounding off 1.0 | 1.1.0 | 2026-11-09 |
| P11, a view's leaves and tiles without thawing | 1.2.0 | 2026-11-10 |
| P12, picking a grid, and fields between topologies | 1.3.0 | 2026-11-12 |
| A fix of 1.3.0, no milestone | 1.3.1 | 2026-11-18 |
| P13, reading only what is needed, and rounding off the issues | 1.4.0 | 2026-11-18 |

### P14 — speed and memory, outputs unchanged

Set by the owner on 2026-11-18, to be released as 1.5.0: faster and smaller, no byte of a
written file and no promised result changed, each change measured by a benchmark before
it lands (TESTING.md T10), a benchmark item added first where none covers the code. Each
item is a GitHub issue of the milestone P14, its `owner-approved` label last applied by
`akahmys` (checked 2026-11-18). In this order, each after what it builds on: 18 before the
spikes on colino's queries, 19 before labelling and the benchmarks it speeds, 20 before 24,
and each spike after the item it is measured against.

- [x] [Issue 18](https://github.com/akahmys/brodo/issues/18): colino's sign only for the leaves kept in the band
- [x] [Issue 19](https://github.com/akahmys/brodo/issues/19): a node's child found by a rank table
- [ ] [Issue 20](https://github.com/akahmys/brodo/issues/20): read buffers allocated zeroed
- [ ] [Issue 21](https://github.com/akahmys/brodo/issues/21): gather by rows
- [ ] [Issue 22](https://github.com/akahmys/brodo/issues/22): colino's Hermite signs, query stacks and candidates
- [ ] [Issue 23](https://github.com/akahmys/brodo/issues/23): colino's parity sign shared along a leaf's lines
- [ ] [Issue 24](https://github.com/akahmys/brodo/issues/24): an archive's ranges, binary16 and whole read
- [ ] [Issue 25](https://github.com/akahmys/brodo/issues/25): labelling by each leaf's components
- [ ] [Issue 26](https://github.com/akahmys/brodo/issues/26): benchmarks for the Euler characteristic, dilation and `.vdb`, and what
      they show
- [ ] B-028, B-029 and B-030 (§3, issues 27 to 29) answered by their spikes and ADRs, before
      anything is written
- [ ] A refactoring of the whole workspace, set by the owner on 2026-11-18: what is
      repeated, named or shaped unevenly across brodo, colino and mestolo, outputs and API
      unchanged, its scope put to the owner before it starts
- [ ] The retrospective and refactoring (PLANNING.md §7), with five additions to the audit
      the owner proposed on 2026-11-21, not taken before it unless the owner brings them
      forward: (1) the hand-written checks and guards tried in every gate on cases of known
      answer, and a check running a tool telling "found", "none" and "did not run" apart;
      (2) no name of the local list of forbidden sources (AGENTS.md S1) in a committed text
      file or commit message, checked without writing the names into the repository;
      (3) the gate run by a `pre-push` hook, refusing uncommitted changes, what is too slow
      moved to the version step; (4) each figure a record quotes from a spike found in that
      spike's output, and how older spikes are treated; (5) a CPU time and memory limit per
      test binary, `NAME.md §N` references resolving, and every file of `scripts/audit/`
      and `scripts/hooks/` described in AUDITING.md. For each: what brodo already does,
      read and tried on known cases before deciding, then a recommendation to take, leave
      or reshape it, with a plan, an estimate and pass conditions written before measuring,
      put to the owner; each taken lands with its ADR (AGENTS.md, "Changing the rules")
- [ ] The closing checks, chosen by the owner
- [ ] Version 1.5.0 (TESTING.md V2–V4)

### Held — `piatto`

Not built for now (owner, 2026-10-24; ADR-0018; §3, B-021). If it is, then:

- [ ] Its dependencies approved and measured against CODING.md Rules 12 and 13 on every
      target (§4); B-013 answered first
- [ ] The first version, and no more (ARCHITECTURE.md §6.2): an isosurface by ray marching,
      a slice moved by the user, a choice of grid; a library and a thin windowed program
- [ ] A `GridView` buffer read on the GPU as it is (TESTING.md §1); what cannot be read is
      recorded as a question on the frozen layout

## 3. Open questions and spikes

Each question is answered by a spike's measurement and an ADR, never in passing by an
implementation (PLANNING.md §2).

| # | Question | Proposal so far |
| :--- | :--- | :--- |
| B-028 | Does contracting the winding number's far-field term once per node pay, and how many voxels' bytes does it change, a sign flipping where the number is within rounding of 1/2 ([Issue 27](https://github.com/akahmys/brodo/issues/27))? | A spike, P14 (owner, 2026-11-18): the time of `convert_*` and the bytes that differ, on closed and open meshes |
| B-029 | Does a 16-bit encoding over a grid's range, or codes by difference along a leaf's rows, store an archive smaller than binary16 or 8 bits within its error ([Issue 28](https://github.com/akahmys/brodo/issues/28))? | A spike, P14 (owner, 2026-11-18): sizes, errors and speeds on `spikes/quantise/`'s grids; a new encoding is a new code of the archive's format |
| B-030 | Does colino's hierarchy split by surface area query faster on meshes of thin triangles, and which bytes near an open mesh's surface does it change ([Issue 29](https://github.com/akahmys/brodo/issues/29))? | A spike, P14 (owner, 2026-11-18): query times and bytes on real meshes |

Questions decided "not now", each revisited only when what it names happens:

| # | Question | Decision |
| :--- | :--- | :--- |
| B-019 | Does `colino` move to a repository of its own? | Not now (owner, 2026-10-23; ADR-0012 keeps two crates in one repository): changes span both, as P3's Hermite format and producer did, and before 1.0 a split would turn each into two releases, duplicate the rules and their audit, and take colino's tests out of brodo's gate. Revisited when brodo's API and formats settle (1.0), both are on crates.io, colino needs what brodo's rules forbid, or others maintain it. Publishing does not split them: both crates are published from this repository (owner, 2026-10-29). A split waits for colino needing what brodo's rules forbid, or for others maintaining it, and is then a milestone of its own that gives colino its own rules, audit and CI (owner, 2026-10-29). Kept before 1.0 (owner, 2026-11-04) |
| B-021 | Does brodo build `piatto`? | Not now (owner, 2026-10-24; ADR-0018): no user has asked for a viewer, the slice as PNG and VTK for ParaView serve checking, `.vdb` from P7 would open in common tools, and a GPU, window and GUI stack is costly. Revisited when a user asks for a viewer or for drawing to embed, or grids too large for VTK and `.vdb` must be looked at |
| B-013 | Which crates handle `piatto`'s window, input and GUI parts? | Left with B-021: `winit` and `egui`, to be measured against Rules 12 and 13 if `piatto` is built (ADR-0018) |
| B-018 | Does model checking with Kani join the closing checks? | Not now (owner, 2026-10-23, kept 2026-11-02 and before 1.0 on 2026-11-04): it fits only integer bit work and `view.rs`'s small helpers, already fuzzed and mutation-tested, and cannot reach rayon or the memory mapping; this milestone's defects were numeric and in design claims. Revisited by a spike if Miri-unreachable unsafe code grows, an integer coordinate bug escapes the tests, or before 1.0 |
| B-031 | Does brodo hold a field's frames over time in one file? | Not now (owner, 2026-11-19, from a study in the session against 1.4.0): a sequence of files, one frame each, already works, each frame replaced whole by the writer, topologies shared by `thaw_on` and moved between by `Remap::between`, a time kept in `GridMeta::extra`. One file of many frames would need a format version written without a count of entries and its directory last (ADR-0038), recovery from a file cut short, a new untrusted reader, and decoding back to a key frame on a random read. It pays only if coding along time, an exclusive or with the frame before compressed by LZ4, stores real sequences much smaller than LZ4 frame by frame; sharing a topology saves about 3 % of an `f32` field by its leaves' masks alone. Measured by nothing. A structural identity in place of the stamp was rejected: a leaf removed and made again leaves the structure as it was, so an unremapped field would keep the removed leaf's values silently (ADR-0014). Revisited when a user asks for frames in one file, or when B-029's spike or issue 36 measures codes along time; a writer needing no count is [Issue 37](https://github.com/akahmys/brodo/issues/37), a format with the directory last, held for the next MAJOR |

Questions answered, each by its spike and ADR:

- `spikes/remap/`: B-001 and B-008 (ADR-0014)
- `spikes/merge/`: B-010 (ADR-0014)
- `spikes/ghost/`: B-002, one to three layers (ADR-0015)
- `spikes/threads/`: B-014, rayon kept (ADR-0015)
- `spikes/archive/`: B-005, `lz4_flex` (ADR-0016)
- `spikes/quantise/`: value quantisation in the archive (ADR-0016's amendment on quantised
  values)
- `spikes/region/`: reading a region of an archive (ADR-0016's amendment on a region)
- `spikes/chunkread/`: B-027, a region and a listing read from only the bytes they need
  (ADR-0016's amendment of 2026-11-16)
- `spikes/vtk/`: B-012 (ADR-0018)
- `spikes/bench/`: B-007 (ADR-0019)
- `spikes/parvar/`: B-024, a benchmark on the performance cores (ADR-0019)
- `spikes/updrift/`: B-026, the leaf-parallel update's drift between sessions (ADR-0019's
  amendment of 2026-11-12)
- `spikes/mutants/`: the audit review's proposal C (ADR-0023)
- `spikes/dilate/`: B-011, no mask (ADR-0025)
- `spikes/sign/`: B-016 (ADR-0027)
- `spikes/thin/` and `spikes/recheck/`: B-020 (ADR-0028)
- `spikes/value/`: B-017 (ADR-0029)
- No spike: B-006, by measurement of `vdb-rs`'s dependencies (ADR-0045)
- `spikes/vdb/`: the writing step (ADR-0046)
- `spikes/vdbread/`: the reading step (ADR-0047)
- `spikes/vdbmemory/`: the memory `vdb::read` makes (ADR-0047's amendment of 2026-11-02)
- `spikes/vdbclass/`: a `.vdb` level set read as an SDF (ADR-0047's amendment of
  2026-11-06)
- No spike: B-003 and B-004, both no, questions of scope (ADR-0012's amendment of
  2026-11-02)

No spike is running.

## 4. Administrative items still open

- Dependencies, each to be approved before it is added (CODING.md Rule 20): after
  P1, for `piatto` if it is built: a GPU, window and GUI stack (B-013).
- The repository is public, since 2026-10-25 (the owner; ADR-0034). Kept so: every
  author and committer the noreply address, no path into a home directory and no name of
  the owner's local list of forbidden sources in any commit (checked 2026-11-02: none of
  its names in any commit's patch or message), the gate's history scans passing, `main`
  protected against force pushes and deletion, and workflows from an outside
  contributor's pull request held for approval. Patents and licences were judged by the
  owner on a preliminary search (ADR-0007), and again for crates.io, unchanged
  (2026-11-04); they are asked again when the form of distribution changes.
- `scripts/hooks/provenance_guard.py` blocks a spike's C++ `#include` of OpenVDB's main
  header, a path naming OpenVDB at no tag, though building against the installed library
  reads none of its source; `spikes/vdbclass/`'s line was added by the owner by hand
  (2026-11-06). Letting such a line pass is a change to a rule, the owner's to decide
  (AGENTS.md, "Changing the rules"), with ADR-0044 amended and the check shown to fire.
- Gaps T14's survey found before 1.0 that a later MINOR adds without breaking a user
  (2026-11-05), split by the owner (2026-11-08): those that round off what is there were
  P10's, released in 1.1.0; those that add a capability are GitHub issues 1 to 12, taken as any issue
  is (AGENTS.md S5).
- Changes that break a user, held for a MAJOR made for a reason of its own, since brodo,
  colino and mestolo share one version (TESTING.md V1): the GitHub issues of the milestone
  "Next MAJOR", issues 30 to 37 (the owner, 2026-11-18 and 2026-11-19), among them mestolo's `info`
  showing Hermite data and encodings and `slice`'s options moved out of `Output` (ADR-0050),
  and a smaller archive codec once `ruzstd` implements levels above its fastest (0.9.1 does
  not; ADR-0016, 2026-10-29).
- Requests from users. Held until one asks: merging into a root tile (ADR-0014);
  labelling many lone leaves faster, each one's upper node costing `label` about 1.3 ms
  and 0.4 MiB of empty places under 6-connectivity (`spikes/rootsparse/REPORT.md`,
  Finding 5; ADR-0013); a public
  way to read a bare frozen buffer, a file of its own, into an aligned buffer, which only
  tests now do (P5's review, S1, 2026-10-27); a bound on a conversion's leaves, refused
  with a typed error before memory runs out, where now the caller bounds the cost by
  choosing h (the review of the promises, 2026-10-28). From a
  user keeping time-stepped fields on brodo's tree, received 2026-10-20 against v0.2.0,
  highest priority first, each with its proposed answer:
  - R2, `[f64; 3]` as a value type, frozen files included: done (ADR-0029).
  - R9, ghost data up to three voxels away: done, `gather_wide`, one to three layers,
    decided by B-002's spike (ADR-0015); reads beyond the window through `ValueAccessor`.
  - R4, two dimensions: answered, no mode of its own; a grid one leaf thick is the
    supported way and is not expected to change (ARCHITECTURE.md §1, ADR-0012).
  - R8, splitting every tile in a region in one edit: answered, met by `split_tile` per leaf
    origin within one `TopologyEditor`, one `Remap` at `commit`; shown by a test and
    documented (ARCHITECTURE.md §4.2). An operation of its own only if a benchmark shows
    the calls cost.
  - R7, vector fields in VTK output: done, `[f32; 3]` and `[f64; 3]` as three-component
    `Vectors` (ADR-0018).
  - R10, several fields of one leaf gathered at once: declined. Looking up the neighbours
    took 1.2 to 2.9 % of a gather, and sharing it gained nothing beyond the pairs' spread
    (`spikes/ghost/REPORT.md`, ADR-0015).
  - Asked, not requested: grids of several resolutions with a defined relation between
    coarse and fine voxels. The design does not decide it: a cache file holds several
    grids, each with its own affine transform, and nothing relates their voxels. The owner
    expects it to be requested, a finer grid where a coarse one is not enough (2026-11-04);
    when it is, a spike and an ADR decide it, on a narrow-band SDF among its cases. An
    agent's reading of the design (2026-11-04), measured by nothing: levels of ordinary
    grids, each half the voxel size of the one above, a fine grid refining whole leaves,
    since a leaf has no per-voxel active state. Open: whether a coarse grid keeps its
    leaves under a fine one, since without them a sparse fine level reads its background
    where a coarse tile held the inside, the SDF's sign wrong; how a level is placed at
    voxel corners or centres, the second's offset not exact in `f64`; a band of coarser
    leaves round each level, so that a gather meets one level only; ghost values from a
    coarser level, which need interpolation, a solver's choice (AGENTS.md S4); an edit
    that moves leaves between levels at once; coarse tiles under fine leaves; fine
    coordinates within `i32`; and the check of a set of levels read as untrusted, its
    cost, its limit and its fuzz target (TESTING.md T6). Whether it is within
    ARCHITECTURE.md §1 is the owner's.
  From a user building cut cells on brodo's grid, received 2026-10-20, answered the same
  day:
  - R11, a group per triangle read from OBJ, STL and 3MF, and Hermite data on the corners
    of an SDF's voxels (signs, and per crossed edge a fraction, normal and group):
    accepted, ADR-0030, stored as `f32`, `[f32; 3]`, `u32` and `u8` fields.
  - Held until asked: a per-triangle table in the cache file, for exact `f64` crossings;
    every crossing along an edge, which the user measured at 0.3 to 1.2 % of crossed
    edges on a turned box (2026-10-20).
  From a user's solver reading callers' cache files as untrusted, received 2026-10-26
  against v0.5.0:
  - R12, a grid read from a cache file turned back into an owned topology and field, at a
    cost that follows the file, not the grid's bounds: accepted, ADR-0042, `GridView::thaw`,
    `thaw_on` and a public `same_topology`, in 0.6.0.
