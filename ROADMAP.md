# Roadmap

Where the work is, what is next, and what is still open. Rules are in the phase documents;
the design is in [ARCHITECTURE.md](ARCHITECTURE.md); versions are in
[CHANGELOG.md](CHANGELOG.md).

## 1. Status

Overwritten at the end of every unit of work (PLANNING.md §1). History is in git.

- **Done**: U3 of `freeze` (ADR-0008 amendment of 2026-10-17): `CacheFile::map` reads a
  cache file by memory mapping through the same parser as `CacheFile::read`; the mapping
  is `crates/brodo/src/mapping.rs`, in `UNSAFE_MODULES`, one unsafe call under a `SAFETY`
  argument. Opening a 40 MB file and reading a value took 3.7 ms by copy and 0.05 ms by
  mapping (fastest of 15, two runs, Apple M5, 2026-10-17; not by TESTING.md T5).
- **Next**: flood fill (§4.5).
- **Unresolved problems**: `cargo-miri` is not installed for nightly on this machine, so
  whether Miri can run a file mapping was not measured; the mapping tests carry
  `ignore` under Miri as ADR-0008 decided. Installing it downloads a toolchain component,
  for the human to allow before the P1 Miri run (T7).

## 2. Milestones

Named after the project group's phases (ADR-0011). Every milestone closes with a
retrospective and refactoring (PLANNING.md §7), a fuzz round with no finding (TESTING.md
T6) and a Miri run (T7), and benchmarks compared with their baselines (TESTING.md §1),
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
- [ ] Two examples in `crates/brodo/examples/`, run by tests, written from textbook
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
- [ ] Flood fill (§4.5); `Grid<T>` was built with the slice (ADR-0014)
- [ ] Checking output (ARCHITECTURE.md §6.1, ADR-0018): a slice as PNG and a field for
      ParaView, behind cargo features; the PNG encoder approved first (§4), B-012 answered
      first, by a spike; golden slice tests (TESTING.md §1)
- [ ] Benchmarks and their first baselines (TESTING.md §1)
- [ ] Retrospective and refactoring; fuzz round; Miri run
- [ ] **Accepted when** flood fill, neighbour traversal and a double-buffered update work,
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

- [ ] Euler characteristic, connected components, dilation and erosion (§4.5)
- [ ] STL, OBJ and 3MF readers, each in `UNTRUSTED_INPUT` and fuzzed (§5.1)
- [ ] Generalised winding number and the narrow-band output (§5.1)
- [ ] Topology report, layers 1 and 2 (§5.2)
- [ ] README and examples for publishing on its own
- [ ] Retrospective and refactoring; fuzz round; Miri run
- [ ] **Accepted when** the invariants are right for a sphere, a torus and a double torus;
      a holed mesh leaks no sign; a wall thinner than a voxel is reported

### After P2

Ordered by users' requests (§4): the archive profile, value quantisation, colino's layer 3,
writing `.vdb`, STEP input.

## 3. Open questions and spikes

Each question is answered by a spike's measurement and an ADR, never in passing by an
implementation (PLANNING.md §2).

| # | Question | Proposal so far |
| :--- | :--- | :--- |
| B-002 | How wide is the ghost layer? | One layer; a macro makes two possible |
| B-003 | Do level-set operations (CSG, redistancing) belong in brodo? | No, until shown to be general |
| B-004 | Does a shared IR schema crate live in this repository? | Undecided; for brodo's users to agree, not for brodo to anticipate |
| B-005 | What is the archive profile's default compressor, and is it pure Rust (CODING.md Rule 12)? | Zstd, LZ4 when decompression speed is asked for; whether a pure-Rust implementation suffices is to be measured |
| B-006 | Does `vdb-rs` pass Rules 12 and 13 (no C, permissive licence) on every target? | To be checked before it is added |
| B-007 | Which harness runs the benchmarks? | None chosen |
| B-012 | Which VTK format does the checking output write: a multiblock of rectilinear grids, one per leaf, or an unstructured grid? | None yet; the reason is recorded with the answer (ADR-0018) |
| B-013 | Which crates handle `piatto`'s window, input and GUI parts? | `winit` and `egui`, to be measured against Rules 12 and 13 (ADR-0018) |
| B-011 | Do dilation, erosion and flood fill (§4.5) need a per-voxel active mask in each leaf? | None yet; a leaf has no per-voxel state now (ADR-0013) |

No spike is running. `spikes/remap/` answered B-001 and B-008, and `spikes/merge/` B-010
(ADR-0014).

## 4. Administrative items still open

- Dependencies for P1, each to be approved before it is added (CODING.md Rule 20): a
  pure-Rust PNG encoder for the checking output (ADR-0018). After
  P1, for `piatto`: a GPU, window and GUI stack (B-013).
- Before any distribution: confirm the licence terms for that form of distribution
  (ADR-0007).
- Before registering on crates.io: `publish`, and each crate's description and keywords
  naming its function (sparse volume, VDB, SDF, level set, voxel). Both names were found
  unregistered on 2026-10-16, per the design.
- Requests from users: none yet. Held until one asks: merging into a root tile (ADR-0014).
