# Roadmap

Where the work is, what is next, and what is still open. Rules are in the phase documents;
the design is in [ARCHITECTURE.md](ARCHITECTURE.md); versions are in
[CHANGELOG.md](CHANGELOG.md).

## 1. Status

Overwritten at the end of every unit of work (PLANNING.md §1). History is in git.

- **Done**: P4 accepted by the human and closed as 0.5.0 (2026-10-25): the API's shape
  (ADR-0035), shown by `api_growth.rs` to take a field or a variant without breaking a user;
  readers and checking output behind features, a read limit, `map` an `unsafe fn`
  (ADR-0036); `docs/formats.md` held by its reader (ADR-0037); the SDF convention's key
  (ADR-0039). Its mutation run, the first on GitHub Actions, missed 148 mutants behind
  opt-in features until the run took every feature (ADR-0023). The repository is public
  (ADR-0034, ADR-0007).
- **Next**: P5 (§2), colino's milestone: its costs measured and those that helped kept
  (2026-10-25), B-016's sign without orientation and layer 3 (B-020) built
  (2026-10-26); next its closing checks.
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
  `cache_write_x8` ran 169 to 214 % slow in the first child of a pair, the second within
  1 %, twice (2026-10-25, 2026-10-26); the first file write of a run is the likely cause,
  not shown.
  Labelling held about 161 MB for the 33 million leaf voxels of a sphere of radius 400,
  near 5 bytes a voxel, and took 2.42 s on one thread (2026-10-26); it has a benchmark
  item. The gate's debug tests grew: `morphology_reference` takes 27 s at a
  load of 5 (2026-10-19).

## 2. Milestones

Named after the project group's phases (ADR-0011). Every milestone closes with a
retrospective and refactoring (PLANNING.md §7), a fuzz round with no finding (TESTING.md
T6), a Miri run (T7) and a mutation run with none missed (T11), and benchmarks compared
with their baselines (TESTING.md §1), before its version is set (TESTING.md V2). From P5
on, the human chooses which of these a milestone closes with, by its content, and its
checklist lists them (ADR-0033).

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

### Held — `piatto`

Not built for now (owner, 2026-10-24; ADR-0018, B-021). If it is, then:

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
- [x] Mutation run (ADR-0023): every mutant of both crates, then the misses and timeouts
      on the code that killed them, then every mutant of `intersect.rs` once its end
      case was fixed; 26 misses killed by tests or listed as equivalent (2026-10-25)
- [x] **Accepted when** ADR-0030's acceptance tests pass and colino's Hermite data keeps
      to brodo's format

### P4 — The public API, before it grows

Scheduled by the human on 2026-10-23, before P5 changes colino's options and report: one
review of what brodo and colino show, so that later additions break no user.

- [x] The first runs of `.github/workflows/audit.yml` and `mutants.yml` (ADR-0034): both
      held (2026-10-25); where P4's mutation run goes, there or on the owner's machine, is
      decided when it closes, a whole run there costing about $82 on a private repository
- [x] An ADR on the API's shape (ADR-0035), the owner choosing every recommended option
- [x] The API reviewed for attack surface too (ADR-0036), the owner choosing every
      recommended option, after Anthropic's article on it (2026):
      colino's STL, OBJ and 3MF readers behind cargo features, 3MF's ZIP and XML parsing
      not built for those who do not read 3MF; brodo's `slice-png` and `vtk` off by default;
      a limit on the bytes `CacheFile::read` takes; `CacheFile::map`'s promise, that the
      file is not changed while mapped, made harder to miss; and whether default
      behaviour reaches a parser or an option weakens a check that nothing needs
- [x] File formats and how users pin brodo, scheduled by the human on 2026-10-25: each
      decided by its own ADR, the owner choosing every recommended option (ADR-0037 to
      ADR-0041); what they ask is built with the API's changes below
  - [x] A specification of the file formats as one document in `docs/`, apart from the
        code: the cache profile (layout version 1, format version 1), Hermite data
        (ADR-0032) and the narrow-band SDF's conventions (below), written so that a reader
        in another language, or one that will not depend on the crates, can read and write
        them from it alone; ARCHITECTURE.md and the ADRs then point to it
  - [x] The formats' version policy: a reader now takes its own version only, and some
        users pass files across process boundaries, so how producers and readers move
        when a version is raised (older versions read as well, refused with an error
        naming the version, or another way)
  - [x] Whether the narrow-band SDF's conventions (where samples lie, which sign is inside,
        the unit of values, the band's width and the value it clamps to, the values of
        tiles and the background) stay colino's choices as a producer (ADR-0027) or
        become a public convention any producer writes, as Hermite data's did. To weigh:
        the Hermite format already assumes the SDF's sampling lattice, its own lattice
        being that one shifted by −h/2; and a producer other than colino, one building
        SDFs from analytic shapes, writes its SDFs to the same conventions (the owner,
        2026-10-24)
  - [x] How users pin brodo: the README says to pin a release tag; and whether, when a
        user needs a feature not yet released, a small release is cut rather than letting
        it pin an arbitrary revision, since a user reaching brodo both directly and
        through another crate in one process gets two distinct copies of its types when
        the two revisions differ
  - [x] (Optional) whether the cache file and a Hermite set get a generic media type name
        users can register or refer to
- [x] The changes ADR-0035 to ADR-0041 ask for: the API's shape, the features and limits,
      `docs/formats.md` with its Python reader, the SDF convention's key, the README's
      pinning; breaking ones listed in CHANGELOG.md together
- [x] Retrospective and refactoring; fuzz round; Miri run; benchmarks; mutation run
      (ADR-0023): all held (2026-10-25)
- [x] **Accepted when** adding a field or an error variant later breaks no code written
      against the API, shown by a test outside the crate: `api_growth.rs` in each crate,
      measured with a variant and a field added (ADR-0035); accepted 2026-10-25

### P5 — colino: a sign without orientation, thin places, costs

Scheduled by the human on 2026-10-23, split from the formats (P6) and ordered on
2026-10-25. Each step that answers an open question is a spike and an ADR before it is
built.

- [x] Costs a review of 2026-10-25 found by reading, each measured on a workload that
      reaches it before anything changes (TESTING.md T10), those that help kept, the others
      recorded as measured; first the two a user's mesh can reach:
  - [x] Hermite data's fallback, where no triangle is hit, scans every triangle for each
        such edge (`colino/src/hermite.rs`): a mesh with holes or overlaps costs
        unhit edges × triangles; a nearest-triangle search through the BVH would not.
        Kept: the BVH's search, its answer the scan's to the bit, took
        `hermite_holed_r50` from 409 to 274–284 ms, as fast as the closed sphere
        (2026-10-25)
  - [x] Conversion's candidate cells test every leaf cell of a triangle's box
        (`colino/src/convert.rs`): a large slanted triangle, a CAD mesh's floor or wall,
        costs its box's volume rather than its area. Not changed: on a slab 6 voxels
        thick turned about two axes, twelve triangles, finding candidates took 1.8 ms of
        a 220 ms conversion at 320 voxels a side, 86 ms of 2.59 s at 1280 and 496 ms of
        10.0 s at 2560 (2.65 GB resident), so that removing it would gain under 10 %
        below about 5000 voxels a side (2026-10-25)
  - [x] `hermite` builds the BVH twice, once itself and once inside `Winding::new`, and
        conversion a third time. Not changed: on the sphere of radius 50, 8192
        triangles, the BVH took 2.3 to 3.3 ms and `Winding::new` 4.9 to 8.1 ms beside
        Hermite data's 340 ms; at radius 400, 131 072 triangles, 28 and 66 ms beside a
        33 s conversion (2026-10-25). `hermite` uses `Winding`'s tree rather than
        building its own, chosen by the human (2026-10-26) to remove a duplicate, not for
        speed; sharing conversion's would widen the API
  - [x] Hermite data's search keeps a `Field<[f64; 3]>` over every leaf for each axis in
        turn, the triangle number carried as the bits of an `f64`, where crossings are
        few. Kept: each leaf's crossings gathered as a list by `par_reduce`, the bytes
        unchanged, took the peak resident size of converting the sphere of radius 200
        (8.3 million leaf voxels) and making its Hermite data from 788 to 613 MB, the
        conversion alone 140 MB (2026-10-25)
  - [x] `label`'s work buffers allocated per node and per call (9.5 s and 440 MB at
        r = 400, 2026-10-20), `LeafManifest::build`'s lookups from the root, and `freeze`'s
        plan rebuilt from sorted coordinates rather than from the tree. Kept for `label`:
        99 % of its time was walking the tree, 49 million of its 51 million
        classifications single voxels each found from the root, most of them in 19 million
        pairs across leaves pushed on the stack one by one; a leaf's voxels are now joined
        from its number, the labels unchanged, `label_sphere_r50` 165 -> 53 ms and the
        sphere of radius 200 2.41 -> 0.63 s (2026-10-25). Kept for `LeafManifest::build`:
        87 to 95 % of `commit`, 26 lookups from the root a leaf; through the accessor's
        path, which keeps the last lower node, 1.27 -> 0.45 ms for 1037 leaves and 20.4 ->
        9.0 ms for 16 307, `activation` 3.33 -> 1.59 ms (2026-10-25). Kept for `freeze`'s
        plan: 70 to 90 % of `freeze`, most of it each tile's number found again from the
        root and every leaf's and tile's node key sorted; tiles now carry their number and
        runs of equal keys are dropped before the sort, the bytes unchanged, `freeze_x8`
        4.00 -> 1.65 ms and the sphere of radius 200 9.0 -> 2.3 ms (2026-10-25). The work
        buffers themselves were not changed: walking the tree, not allocating, took the
        time. At r = 400, labelling then took 2.44 s and still about 436 MB (2026-10-25);
        labels stored as `u32` and written over the union-find's parents, chosen by the
        human (2026-10-26), took that to 161 MB, the labels unchanged (2026-10-26)
- [x] B-016: a sign that does not depend on orientation, by a spike on synthetic broken
      meshes (`spikes/sign/`), as an option of conversion and Hermite data: the parity
      count along the axes, `Sign::Parity` (ADR-0027). On the sphere of radius 50 it
      converted in 165 to 187 ms and made Hermite data in 207 to 217 ms, beside the winding
      number's 300 to 382 and 320 to 369 ms, at a load near 6 (2026-10-26)
- [x] colino's layer 3, places thinner or gaps narrower than two voxels (B-020)
  - [x] Finding them, by rays along each sampled point's normal (ADR-0028): the report of
        the sphere of radius 50 took 117 ms with it, 84 without; of radius 200, 2.16 s and
        1.69 s; of radius 400, 9.5 s, at loads near 3 (2026-10-26)
  - [x] Checking them by a local reconversion at h/2 (ADR-0028): the three invariants of
        each place's box at h and at h/2, a box beyond 2²⁶ voxels at h/2 unchecked
- [ ] Retrospective and refactoring; benchmarks; mutation run (ADR-0023). Chosen by the
      human for this content (ADR-0033): no fuzz round or Miri run, unless a step adds a
      reader of untrusted bytes or unsafe code
  - [x] Retrospective, its proposals decided by the human (2026-10-26); refactoring:
        colino's vector helpers in `distance.rs`, the tree's box in `Bvh::bounds`
  - [x] Benchmarks within their baselines, both crates, at loads of 1.7 and 1.5
        (2026-10-26)
  - [ ] A review of the logic and algorithms of both crates, chosen by the human
        (2026-10-26), what P5 wrote or rewrote first; each defect fixed on its own, with
        its test. Three passes, by module, by concern and by workflow, found 53 items
        (2026-10-26): readers of untrusted bytes that hang or exhaust memory on small
        files (3MF, cache, Hermite, OBJ), parts sharing a face split in two, a Hermite set
        written beside its SDF claiming the SDF's convention, and costs beyond the band
  - [ ] A review again of what the fixes changed, chosen by the human (2026-10-26)
  - [x] R12, a frozen view thawed into a topology and a field (ADR-0042), scheduled by
        the human (2026-10-26), after the review's fixes to the Hermite reader: landed
        (2026-10-27), `GridView::thaw`, `thaw_on` and `same_topology`
  - [ ] Refactoring across the crates, its candidates listed for the human to choose
  - [ ] The public API tidied in 0.6.0, already breaking for `Stored`:
        `FreezeError::Inconsistent` removed among it
  - [ ] Mutation run, after the review and the refactoring; a run on `9940bd5`
        (2026-10-26) shows where tests are weak
- [ ] **Accepted when** a badly oriented mesh converts with the new sign, layer 3 finds
      thin walls and narrow gaps, and each cost above is measured, kept where it helps

### P6 — brodo: archives, quantisation, OpenVDB files

Split from P5 by the human on 2026-10-25. Each step is a spike and an ADR before it is
built.

- [ ] B-005: the archive's compressor, pure Rust, its dependency approved
- [ ] The archive profile (ARCHITECTURE.md §4.7), its reader in `UNTRUSTED_INPUT` and fuzzed
- [ ] Value quantisation in the archive, 16-bit float and 8-bit range, with byte shuffling
- [ ] `.vdb`: B-006 for reading through `vdb-rs`; writing only if the format can be learnt
      within AGENTS.md S1, as the human decides
- [ ] Retrospective and refactoring; fuzz round, the new readers included; Miri run;
      benchmarks, the archive's with baselines; mutation run (ADR-0023), as the human
      chooses (ADR-0033)
- [ ] **Accepted when** archives round-trip within their stated error, and `.vdb` files
      are read as B-006 decides

### After P6

STEP is not read (ADR-0031). `piatto` is held (§2, B-021).

## 3. Open questions and spikes

Each question is answered by a spike's measurement and an ADR, never in passing by an
implementation (PLANNING.md §2).

| # | Question | Proposal so far |
| :--- | :--- | :--- |
| B-023 | Does Hermite data hold its crossings sparsely, only on the edges that hold one, rather than nine dense fields over every leaf? | Not yet: found by P5's review (M2): the fields take about 61 bytes a voxel and writing them about three times as much, so that a sphere of radius 1500 voxels does not fit in 32 GB, read against a measurement of 613 MB at radius 200 (2026-10-26). It changes the Hermite format, so it waits for a spike and a version raised (ADR-0038); the owner set it aside for P5 (2026-10-27) |
| B-022 | Does a later version of the frozen layout and the cache profile check what version 1's readers leave unchecked: padding and the background's bytes beyond its size, the order of nodes, whether every node and leaf is reached, and a cache file's bounds against its grid (docs/formats.md §2.5, §3.4)? | Not yet: found writing the format document (ADR-0037); each check a reader adds refuses files version 1 accepts, so it waits for a version raised for another reason (ADR-0038). Now for `GridView::new` and the cache reader alone: the Hermite reader and thawing refuse nodes reached twice and leaves never reached at version 1 (ADR-0038's amendment of 2026-10-27, ADR-0042). For the links, one pass would do: each node's `first_child` and `first_tile`, where it has children or tiles, the end of the previous such node's, the last end the count, so that no child is shared or unreached |
| B-019 | Does `colino` move to a repository of its own? | Not now (owner, 2026-10-23; ADR-0012 keeps two crates in one repository): changes span both, as P3's Hermite format and producer did, and before 1.0 a split would turn each into two releases, duplicate the rules and their audit, and take colino's tests out of brodo's gate. Revisited when brodo's API and formats settle (1.0), both are on crates.io, colino needs what brodo's rules forbid, or others maintain it |
| B-021 | Does brodo build `piatto`? | Not now (owner, 2026-10-24; ADR-0018): no user has asked for a viewer, the slice as PNG and VTK for ParaView serve checking, `.vdb` from P6 would open in common tools, and a GPU, window and GUI stack is costly. Revisited when a user asks for a viewer or for drawing to embed, grids too large for VTK and `.vdb` must be looked at, or P6 finds `.vdb` cannot be written |
| B-018 | Does model checking with Kani join the closing checks? | Not now (owner, 2026-10-23): it fits only integer bit work and `view.rs`'s small helpers, already fuzzed and mutation-tested, and cannot reach rayon or the memory mapping; this milestone's defects were numeric and in design claims. Revisited by a spike if Miri-unreachable unsafe code grows, an integer coordinate bug escapes the tests, or before 1.0 |
| B-003 | Do level-set operations (CSG, redistancing) belong in brodo? | No, until shown to be general |
| B-004 | Does a shared IR schema crate live in this repository? | Undecided; for brodo's users to agree, not for brodo to anticipate |
| B-005 | What is the archive profile's default compressor, and is it pure Rust (CODING.md Rule 12)? | Zstd, LZ4 when decompression speed is asked for; whether a pure-Rust implementation suffices is to be measured |
| B-006 | Does `vdb-rs` pass Rules 12 and 13 (no C, permissive licence) on every target, and is it written apart from OpenVDB's source rather than translated from it, which would carry MPL-2.0 (ADR-0007)? | To be checked before it is added |
| B-013 | Which crates handle `piatto`'s window, input and GUI parts? | `winit` and `egui`, to be measured against Rules 12 and 13 (ADR-0018) |

`spikes/sign/` answered B-016 (ADR-0027); `spikes/thin/` and `spikes/recheck/` B-020 (ADR-0028); `spikes/value/` B-017 (ADR-0029). No spike is running. `spikes/remap/` answered B-001 and B-008, and `spikes/merge/` B-010
(ADR-0014); `spikes/vtk/` answered B-012 (ADR-0018); `spikes/bench/` B-007 (ADR-0019); `spikes/threads/` B-014, rayon kept (ADR-0015); `spikes/dilate/` B-011, no mask (ADR-0025); `spikes/ghost/` B-002, one to three layers (ADR-0015); `spikes/mutants/` the audit review's proposal C (ADR-0023).

## 4. Administrative items still open

- Dependencies, each to be approved before it is added (CODING.md Rule 20): after
  P1, for `piatto` if it is built: a GPU, window and GUI stack (B-013).
- Before any distribution: confirm the licence terms for that form of distribution
  (ADR-0007).
- Making the repository public, which the owner decides later (2026-10-24). Readiness is
  kept up meanwhile; checked 2026-10-24: every author and committer is the noreply
  address, no path into a home directory is in the history, the gate's history scans
  pass, and the workflows start by hand only (ADR-0034). Once the owner decides, in order:
  1. Patents: whether the methods brodo implements from papers, the VDB tree among them,
     are under patent, looked into by the owner, with advice if needed.
  2. Licences: the terms confirmed for this form of distribution (ADR-0007); `cargo deny`
     already holds every dependency to the allow-list.
  3. History: the two names of the owner's local list of forbidden sources that the first
     commits carried, removed at `3928db9` (2026-10-20), rewritten out of every commit
     before it. `git filter-repo`, approved first, runs in a fresh clone with
     `--replace-text` and its table kept outside the repository; a search of every
     commit for the two names then finds none; the gate passes; the tags `v0.1.0` and
     later are set again on the rewritten commits; the owner force-pushes `main` and the
     tags, and any other clone is made afresh.
  4. Settings on GitHub, by the owner: the visibility made public; `main` protected
     against force pushes and deletion; workflows from forks' pull requests held for
     approval.
  5. CI: with standard runners free for a public repository, whether `audit.yml` also runs
     on each push, decided then (ADR-0034).

  Done on 2026-10-25, the owner having decided to publish: step 3, the history rewritten
  and force-pushed, the old workflow runs deleted; step 5, `audit.yml` on each push to
  `main` (ADR-0034); the repository made public by the owner; step 4, `main` protected
  against force pushes and deletion for everyone, and workflows from every outside
  contributor's pull request held for approval. Steps 1 and 2, patents and licences, judged by the owner on a
  preliminary search (ADR-0007); asked again when the form of distribution changes.
- Before registering on crates.io: `publish`, and each crate's description and keywords
  naming its function (sparse volume, VDB, SDF, level set, voxel). Both names were found
  unregistered on 2026-10-16, per the design.
- Requests from users. Held until one asks: merging into a root tile (ADR-0014); a public
  way to read a bare frozen buffer, a file of its own, into an aligned buffer, which only
  tests now do (P5's review, S1, 2026-10-27). From a
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
  From a user's solver reading callers' cache files as untrusted, received 2026-10-26
  against v0.5.0:
  - R12, a grid read from a cache file turned back into an owned topology and field, at a
    cost that follows the file, not the grid's bounds: accepted, ADR-0042, `GridView::thaw`,
    `thaw_on` and a public `same_topology`, in 0.6.0.
