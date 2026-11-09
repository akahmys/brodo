# Roadmap

Where the work is, what is next, and what is still open. Rules are in the phase documents;
the design is in [ARCHITECTURE.md](ARCHITECTURE.md); versions are in
[CHANGELOG.md](CHANGELOG.md).

## 1. Status

Overwritten at the end of every unit of work (PLANNING.md §1). History is in git.

- **Done**: P12's issue 2, an archive listed without decompressing a chunk and grids found
  by name in a cache file, an archive and a `.vdb` (ADR-0016, ADR-0055); before it, P11
  released as 1.2.0 (2026-11-10).
- **Next**: P12's issue 7 (§2): a `Remap` between any two topologies, its API put to the
  owner first.
- **Unresolved**: `leaf_parallel_update_x32` drifts between processes by up to 20 %, its
  cause not measured (ADR-0019).

## 2. Milestones

Named after the project group's phases (ADR-0011); the checks a milestone closes with are
TESTING.md §2's and its checklist lists them (ADR-0033). A closed milestone leaves this
file: what it brought is in its version's CHANGELOG.md section, and its checklist in git,
in this file as of `53f69d5` (P0 to P9), `711cf5b` (P10) and `e27fd49` (P11).

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

### P12 — picking a grid, and fields between topologies

Set by the owner on 2026-11-09, started on 2026-11-10, to be released as 1.3.0, adding to
the API without breaking a user. In this order; each change's API is put to the owner
before it is written, its ADR amended where the design changes. Each issue's
`owner-approved` label was last applied by `akahmys` (checked 2026-11-10).

- [x] [Issue 2](https://github.com/akahmys/brodo/issues/2): an archive listed without
      decompressing a chunk, §8.6's rules 1 and 2 held for every entry, and grids found by
      name in a cache file, an archive and a `.vdb`, every match in the file's order
      (ADR-0016, ADR-0055)
- [ ] [Issue 7](https://github.com/akahmys/brodo/issues/7): a `Remap` between any two
      topologies, values moved by `Field::remap`, a tile over leaves and background merged
      by the caller's rule (ADR-0014)
- [ ] [Issue 8](https://github.com/akahmys/brodo/issues/8): the union of two topologies,
      the finer kept, a tile split where the other holds a leaf or a smaller tile, its
      remaps made by issue 7's
- [ ] [Issue 4](https://github.com/akahmys/brodo/issues/4): a writer taking its entry count
      first and one grid at a time, to a path only; `write` and `to_bytes` made by it, their
      bytes unchanged
- [ ] [Issue 3](https://github.com/akahmys/brodo/issues/3): `.vdb` grids of `[f32; 3]` and
      `[f64; 3]`, after a spike against OpenVDB 14 (AGENTS.md S1); `u8` and `u32` decided
      once the spike has looked for their OpenVDB types; mestolo's `convert` taking them
- [ ] [Issue 6](https://github.com/akahmys/brodo/issues/6): B-025 answered by its spike and
      ADR before it is implemented
- [ ] The closing checks, chosen by the owner when the issues are done
- [ ] Version 1.3.0 (TESTING.md V2–V4)

### Held — `piatto`

Not built for now (owner, 2026-10-24; ADR-0018; §3, B-021). If it is, then:

- [ ] Its dependencies approved and measured against CODING.md Rules 12 and 13 on every
      target (§4); B-013 answered first
- [ ] The first version, and no more (ARCHITECTURE.md §6.2): an isosurface by ray marching,
      a slice moved by the user, a choice of grid; a library and a thin windowed program
- [ ] A `GridView` buffer read on the GPU as it is (TESTING.md §1); what cannot be read is
      recorded as a question on the frozen layout

### Later

- [ ] Reading a region of an archive from only the bytes of the chunks it needs, the file
      not read whole (left to later by the human, 2026-10-29; ADR-0016)

## 3. Open questions and spikes

Each question is answered by a spike's measurement and an ADR, never in passing by an
implementation (PLANNING.md §2).

| # | Question | Proposal so far |
| :--- | :--- | :--- |
| B-025 | How does labelled connectivity number the root's empty regions without the dense grid of 2²² cells that refuses scattered leaves as `TooSpread` (issue 6)? | None yet: a spike measures a sparse form of the root's regions against the dense one on the layouts it accepts, before an ADR |

Questions decided "not now", each revisited only when what it names happens:

| # | Question | Decision |
| :--- | :--- | :--- |
| B-019 | Does `colino` move to a repository of its own? | Not now (owner, 2026-10-23; ADR-0012 keeps two crates in one repository): changes span both, as P3's Hermite format and producer did, and before 1.0 a split would turn each into two releases, duplicate the rules and their audit, and take colino's tests out of brodo's gate. Revisited when brodo's API and formats settle (1.0), both are on crates.io, colino needs what brodo's rules forbid, or others maintain it. Publishing does not split them: both crates are published from this repository (owner, 2026-10-29). A split waits for colino needing what brodo's rules forbid, or for others maintaining it, and is then a milestone of its own that gives colino its own rules, audit and CI (owner, 2026-10-29). Kept before 1.0 (owner, 2026-11-04) |
| B-021 | Does brodo build `piatto`? | Not now (owner, 2026-10-24; ADR-0018): no user has asked for a viewer, the slice as PNG and VTK for ParaView serve checking, `.vdb` from P7 would open in common tools, and a GPU, window and GUI stack is costly. Revisited when a user asks for a viewer or for drawing to embed, or grids too large for VTK and `.vdb` must be looked at |
| B-013 | Which crates handle `piatto`'s window, input and GUI parts? | Left with B-021: `winit` and `egui`, to be measured against Rules 12 and 13 if `piatto` is built (ADR-0018) |
| B-018 | Does model checking with Kani join the closing checks? | Not now (owner, 2026-10-23, kept 2026-11-02 and before 1.0 on 2026-11-04): it fits only integer bit work and `view.rs`'s small helpers, already fuzzed and mutation-tested, and cannot reach rayon or the memory mapping; this milestone's defects were numeric and in design claims. Revisited by a spike if Miri-unreachable unsafe code grows, an integer coordinate bug escapes the tests, or before 1.0 |

Questions answered, each by its spike and ADR:

- `spikes/remap/`: B-001 and B-008 (ADR-0014)
- `spikes/merge/`: B-010 (ADR-0014)
- `spikes/ghost/`: B-002, one to three layers (ADR-0015)
- `spikes/threads/`: B-014, rayon kept (ADR-0015)
- `spikes/archive/`: B-005, `lz4_flex` (ADR-0016)
- `spikes/quantise/`: value quantisation in the archive (ADR-0016's amendment on quantised
  values)
- `spikes/region/`: reading a region of an archive (ADR-0016's amendment on a region)
- `spikes/vtk/`: B-012 (ADR-0018)
- `spikes/bench/`: B-007 (ADR-0019)
- `spikes/parvar/`: B-024, a benchmark on the performance cores (ADR-0019)
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
- A smaller codec for the archive: `ruzstd` measured again as `spikes/archive/` did, once
  it implements levels above its fastest (0.9.1 does not; ADR-0016, 2026-10-29).
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
- For the next MAJOR, since it changes an output 1.0 promises (ARCHITECTURE.md §6.3,
  TESTING.md V9): Hermite data and an archive's encodings shown by mestolo's `info`
  (the owner, 2026-11-08; ADR-0050).
- Requests from users. Held until one asks: merging into a root tile (ADR-0014); a public
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
