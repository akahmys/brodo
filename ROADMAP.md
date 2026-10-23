# Roadmap

Where the work is, what is next, and what is still open. Rules are in the phase documents;
the design is in [ARCHITECTURE.md](ARCHITECTURE.md); versions are in
[CHANGELOG.md](CHANGELOG.md).

## 1. Status

Overwritten at the end of every unit of work (PLANNING.md §1). History is in git.

- **Done**: R11 but its fuzz round: groups read from OBJ, STL and 3MF (ADR-0030); Hermite
  data as a brodo format (ADR-0032); `colino::intersect`, watertight in `f64`; and
  `colino::hermite`, ADR-0030's acceptance tests passing (2026-10-22).
- **Next**: P3's mutation run, in which why the two `Winding::approximate` mutants that
  loop for ever passed 60 s without a failed test (2026-10-22) is found out; then its
  acceptance and version 0.4.0.
- **Unresolved problems**: `cache_map_x256` runs 8 to 10 % behind its baseline of
  2026-10-17 at v0.3.0 and now alike, run in turn (2026-10-23); a first run straight after
  the fuzz round and Miri failed at +13 %. Machine drift is the likely cause, not shown. Miri's Stacked Borrows reports undefined behaviour in
  `crossbeam-epoch` under rayon; Tree Borrows does not (ADR-0015), but Miri's nightly of
  2026-07-21 warns there of integer-to-pointer casts in `crossbeam-epoch` 0.9.21, which
  Tree Borrows does not support, so it may miss a bug past them (2026-10-20). The leaf-parallel
  update's benchmark disagrees within a pair by up to 17 %; whether a load average of 3.0
  catches other work is unknown (ADR-0019); readings: other work in another project
  raised it to 14.76 (2026-10-19); a program using more than one core let the run start
  and failed the leaf-parallel update at +16 % (2026-10-20); the Claude app's own drawing
  held the load average near 3 during runs (2026-10-20). Allocating and dropping its
  output field takes about a third of that item (1.04 of 3.3 ms, 2026-10-18, load 3.3),
  but the update alone varied as much, so it is not shown to cause the spread.
  Labelling held about 440 MB for the 33 million leaf voxels of a sphere of radius 400,
  near the 13 bytes a voxel estimated, and took 9.5 s on one thread (2026-10-20); it has
  a benchmark item. The gate's debug tests grew: `morphology_reference` takes 27 s at a
  load of 5 (2026-10-19).

## 2. Milestones

Named after the project group's phases (ADR-0011). Every milestone closes with a
retrospective and refactoring (PLANNING.md §7), a fuzz round with no finding (TESTING.md
T6), a Miri run (T7) and a mutation run with none missed (T11), and benchmarks compared with their baselines (TESTING.md §1),
before its version is set (TESTING.md V2).

### P0 — Repository with its rules

- [x] The rules on sources, in AGENTS.md (S1–S4), and PROVENANCE.md
- [x] `brodo` and `colino` crates, listed in `scripts/audit/project.py`
- [x] Licence: MIT OR Apache-2.0 (ADR-0010)
- [x] The first commit, with the gate passing
- [x] Retrospective on the harness (PLANNING.md §7)
- [x] Version 0.1.0 in `Cargo.toml` and CHANGELOG.md, tag `v0.1.0` (TESTING.md V2–V4)
- [x] **Accepted when** the repository exists with its rules and the gate passes

### P1 — `brodo`

P1 starts with a thin vertical slice, to show that a shared topology and `Remap` suit both
kinds of user before the rest is built on them.

**First: the spike and the slice**

- [x] Dependencies for the slice approved by the human (CODING.md Rule 20): see §4
- [x] Measure that `zerocopy`'s derives pass `unsafe_code = "deny"`, which ADR-0008
      assumes; if not, amend ADR-0008 before `freeze`
- [x] Spike `spikes/remap/` answers B-001 and B-008 by comparing designs; an ADR records
      the answer
- [x] The minimal tree: root and three levels, activation, and splitting a tile into a
      leaf (ARCHITECTURE.md §4.1)
- [x] `Topology` and `Field<T>`; `TopologyEditor`, `commit` and `Remap` as the ADR decides
      (§4.2)
- [x] `LeafManifest`: Morton order and 26 neighbours (§4.3)
- [x] The leaf-parallel update and gather (§4.4)
- [x] Reference tier: property tests against a dense grid (TESTING.md §1)
- [x] Two examples in `crates/brodo/examples/`, run by tests, written from textbook
      material (AGENTS.md S1):
  - [x] Diffusion over several fields, double-buffered, with the leaf-parallel update and
        gather; checked against the analytic solution for a Gaussian, and for conservation
  - [x] A sphere SDF and the union of two, on one `Grid<T>`; checked against exact
        distances. The union is written in the example only (ADR-0012)
- [x] **Slice accepted when** both examples pass their checks and needed no workaround of
      the API. A workaround is recorded and sends the design back through an ADR

**Then, in this order**

- [x] Tiles in full: merging, and tiles in the examples (§4.1); B-010 answered by
      `spikes/merge/` (ADR-0014)
- [x] `ValueAccessor` (§4.4)
- [x] `freeze` and `GridView` (§4.6); the cache profile (§4.7), its reader in
      `UNTRUSTED_INPUT` and fuzzed, its mapping module in `UNSAFE_MODULES`
- [x] Flood fill (§4.5); `Grid<T>` was built with the slice (ADR-0014)
- [x] Checking output, V1 (ARCHITECTURE.md §6.1, ADR-0018): a slice as PNG, feature
      `slice-png` on by default, `png` approved; golden slice tests (TESTING.md §1)
- [x] Checking output, V2: a field for ParaView as one ASCII `.vtu` (B-012 answered by
      `spikes/vtk/`, ADR-0018)
- [x] Benchmarks and their first baselines (TESTING.md §1)
- [x] Retrospective and refactoring; fuzz round; Miri run (ADR-0008)
- [x] **Accepted when** flood fill, neighbour traversal and a double-buffered update work,
      the Reference and Format tiers for these parts pass, at least one golden slice test
      passes, and both examples still pass

### After P1 — `piatto`

Not part of P1's acceptance, and not started while ParaView suffices (ADR-0018).

- [ ] Its dependencies approved and measured against CODING.md Rules 12 and 13 on every
      target (§4); B-013 answered first
- [ ] The first version, and no more (ARCHITECTURE.md §6.2): an isosurface by ray marching,
      a slice moved by the user, a choice of grid; a library and a thin windowed program
- [ ] A `GridView` buffer read on the GPU as it is (TESTING.md §1); what cannot be read is
      recorded as a question on the frozen layout

### P2 — `colino`

- [x] Euler characteristic, connected components, dilation and erosion (§4.5)
- [x] STL, OBJ and 3MF readers, each in `UNTRUSTED_INPUT` and fuzzed (§5.1)
- [x] Generalised winding number and the narrow-band output (§5.1)
- [x] Topology report, layers 1 and 2 (§5.2)
- [x] README and examples for publishing on its own: `crates/colino/README.md`, its code a
      doctest; `torus` and `vent` in `crates/colino/examples/`, run by tests and written
      from textbook material, both needing no workaround of the API
- [x] Retrospective and refactoring; fuzz round; Miri run (ADR-0008); mutation run
      (ADR-0023)
- [x] **Accepted when** the invariants are right for a sphere, a torus and a double torus;
      a holed mesh leaks no sign; a wall thinner than a voxel is reported

### P3 — R11: triangle groups and Hermite data

Scheduled by the human on 2026-10-22, from a user's request (§4, R11).

- [x] A group per triangle, read from OBJ, STL and 3MF (ADR-0030)
- [x] Hermite data as a brodo format any producer writes, with its check (ADR-0032)
- [x] `colino::intersect`, watertight in `f64`, and `colino::hermite` producing the format
- [x] Hermite data from a badly oriented mesh: refused where its sign changes beyond the
      band, counted as unhit within it (ADR-0030); B-016 is the fix
- [x] Retrospective (ADR-0001, -0003, -0006, -0020) and refactoring
- [x] Fuzz round, six targets 901 s each, structured ones seeded, no finding; Miri run, no
      report; benchmarks within their baselines (2026-10-23)
- [ ] Mutation run (ADR-0023)
- [ ] **Accepted when** ADR-0030's acceptance tests pass and colino's Hermite data keeps
      to brodo's format

### P4 — The public API, before it grows

Scheduled by the human on 2026-10-23, before P5 changes colino's options and report: one
review of what brodo and colino show, so that later additions break no user.

- [ ] An ADR on the API's shape: `#[non_exhaustive]` on error enums and on structs that
      grow (`Options`, `Report`, `Limits`, `Mesh` and the like), with constructors; which
      of colino's parts (`bvh`, `winding`, `distance`, `intersect`) stay public
- [ ] The changes, breaking ones listed in CHANGELOG.md together
- [ ] Retrospective and refactoring; fuzz round; Miri run; benchmarks; mutation run
      (ADR-0023)
- [ ] **Accepted when** adding a field or an error variant later breaks no code written
      against the API, shown by a test outside the crate

### P5 — Archives, quantisation, OpenVDB files, local risk, a sign without orientation

Scheduled by the human on 2026-10-23: what was left after P3, with B-016. Each step is a
spike and an ADR before it is built.

- [ ] B-016: a sign that does not depend on orientation, by a spike on synthetic broken
      meshes (`spikes/sign/`), as an option of conversion and Hermite data
- [ ] colino's layer 3, places thinner or gaps narrower than two voxels (B-020)
- [ ] B-005: the archive's compressor, pure Rust, its dependency approved
- [ ] The archive profile (ARCHITECTURE.md §4.7), its reader in `UNTRUSTED_INPUT` and fuzzed
- [ ] Value quantisation in the archive, 16-bit float and 8-bit range, with byte shuffling
- [ ] `.vdb`: B-006 for reading through `vdb-rs`; writing only if the format can be learnt
      within AGENTS.md S1, as the human decides
- [ ] Retrospective and refactoring; fuzz round, the new readers included; Miri run;
      benchmarks, the archive's with baselines; mutation run (ADR-0023)
- [ ] **Accepted when** a badly oriented mesh converts with the new sign, layer 3 finds
      thin walls and narrow gaps, and archives round-trip within their stated error

### After P5

STEP is not read (ADR-0031). `piatto` waits after P1 (§2).

## 3. Open questions and spikes

Each question is answered by a spike's measurement and an ADR, never in passing by an
implementation (PLANNING.md §2).

| # | Question | Proposal so far |
| :--- | :--- | :--- |
| B-019 | Does `colino` move to a repository of its own? | Not now (owner, 2026-10-23; ADR-0012 keeps two crates in one repository): changes span both, as P3's Hermite format and producer did, and before 1.0 a split would turn each into two releases, duplicate the rules and their audit, and take colino's tests out of brodo's gate. Revisited when brodo's API and formats settle (1.0), both are on crates.io, colino needs what brodo's rules forbid, or others maintain it |
| B-018 | Does model checking with Kani join the closing checks? | Not now (owner, 2026-10-23): it fits only integer bit work and `view.rs`'s small helpers, already fuzzed and mutation-tested, and cannot reach rayon or the memory mapping; this milestone's defects were numeric and in design claims. Revisited by a spike if Miri-unreachable unsafe code grows, an integer coordinate bug escapes the tests, or before 1.0 |
| B-017 | Is `Value` sealed, so that only brodo assigns type codes? | Not yet: a type outside brodo is refused by every other build's reader, and sealing would forbid types used in memory only (ADR-0029) |
| B-020 | How does colino's layer 3 find places thinner, or gaps narrower, than two voxels before conversion, and check them by a local reconversion at h/2 (ARCHITECTURE.md §5.2)? | None yet; to be read from papers and answered by a spike in P5 |
| B-016 | What is colino's orientation-independent sign (ADR-0017)? | None yet; candidates: flooding from the outside through cells the surface does not cross, or the winding number's magnitude. Answered in P5 by a spike on synthetic broken meshes, the cube with a face turned in among them (ADR-0030) |
| B-003 | Do level-set operations (CSG, redistancing) belong in brodo? | No, until shown to be general |
| B-004 | Does a shared IR schema crate live in this repository? | Undecided; for brodo's users to agree, not for brodo to anticipate |
| B-005 | What is the archive profile's default compressor, and is it pure Rust (CODING.md Rule 12)? | Zstd, LZ4 when decompression speed is asked for; whether a pure-Rust implementation suffices is to be measured |
| B-006 | Does `vdb-rs` pass Rules 12 and 13 (no C, permissive licence) on every target? | To be checked before it is added |
| B-013 | Which crates handle `piatto`'s window, input and GUI parts? | `winit` and `egui`, to be measured against Rules 12 and 13 (ADR-0018) |

No spike is running. `spikes/remap/` answered B-001 and B-008, and `spikes/merge/` B-010
(ADR-0014); `spikes/vtk/` answered B-012 (ADR-0018); `spikes/bench/` B-007 (ADR-0019); `spikes/threads/` B-014, rayon kept (ADR-0015); `spikes/dilate/` B-011, no mask (ADR-0025); `spikes/ghost/` B-002, one to three layers (ADR-0015); `spikes/mutants/` the audit review's proposal C (ADR-0023).

## 4. Administrative items still open

- Dependencies, each to be approved before it is added (CODING.md Rule 20): after
  P1, for `piatto`: a GPU, window and GUI stack (B-013).
- Before any distribution: confirm the licence terms for that form of distribution
  (ADR-0007).
- Before registering on crates.io: `publish`, and each crate's description and keywords
  naming its function (sparse volume, VDB, SDF, level set, voxel). Both names were found
  unregistered on 2026-10-16, per the design.
- Requests from users. Held until one asks: merging into a root tile (ADR-0014). From a
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
    grids, each with its own affine transform, and nothing relates their voxels.
  From a user building cut cells on brodo's grid, received 2026-10-20, answered the same
  day:
  - R11, a group per triangle read from OBJ, STL and 3MF, and Hermite data on the corners
    of an SDF's voxels (signs, and per crossed edge a fraction, normal and group):
    accepted, ADR-0030, stored as `f32`, `[f32; 3]`, `u32` and `u8` fields.
  - Held until asked: a per-triangle table in the cache file, for exact `f64` crossings;
    every crossing along an edge, which the user measured at 0.3 to 1.2 % of crossed
    edges on a turned box (2026-10-20).
