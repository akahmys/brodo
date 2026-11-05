# Roadmap

Where the work is, what is next, and what is still open. Rules are in the phase documents;
the design is in [ARCHITECTURE.md](ARCHITECTURE.md); versions are in
[CHANGELOG.md](CHANGELOG.md).

## 1. Status

Overwritten at the end of every unit of work (PLANNING.md §1). History is in git.

- **Done**: P9's first units: B-018 and B-019 kept, patents and licences judged unchanged
  (ADR-0007), version 2's media types stated (ADR-0041), golden files of version 2's
  bare frozen buffers written, the tests run on macOS and Windows (ADR-0034), and keys
  beginning `brodo.` reserved, Hermite data at version 3 and the SDF convention at
  version 2 (ADR-0053), the public API reviewed, and the survey's small items taken and
  version 1 of the frozen layout, cache and archive dropped (ADR-0054).
- **Next**: the rest of the survey of designs grown by accretion (2026-11-05), the readers'
  and writers' shapes first; then the rest of P9 in the order put to the owner
  (2026-11-04).

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

Not built for now (owner, 2026-10-24; ADR-0018; §3, B-021). If it is, then:

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
        Kept: the BVH's search, its answer the scan's to the bit, as fast as the closed
        sphere (2026-10-25; measured in commit `6baadc4`)
  - [x] Conversion's candidate cells test every leaf cell of a triangle's box
        (`colino/src/convert.rs`): a large slanted triangle, a CAD mesh's floor or wall,
        costs its box's volume rather than its area. Not changed: removing it would gain
        under 10 % below about 5000 voxels a side (2026-10-25; measured in commit
        `82af279`)
  - [x] `hermite` builds the BVH twice, once itself and once inside `Winding::new`, and
        conversion a third time. Not changed for speed, a few milliseconds beside
        Hermite data's hundreds (2026-10-25; measured in commit `82af279`); `hermite`
        uses `Winding`'s tree rather than building its own, chosen by the human
        (2026-10-26) to remove a duplicate; sharing conversion's would widen the API
  - [x] Hermite data's search keeps a `Field<[f64; 3]>` over every leaf for each axis in
        turn, the triangle number carried as the bits of an `f64`, where crossings are
        few. Kept: each leaf's crossings gathered as a list by `par_reduce`, the bytes
        unchanged, the peak resident size lower (2026-10-25; measured in commit
        `82af279`)
  - [x] `label`'s work buffers allocated per node and per call, `LeafManifest::build`'s
        lookups from the root, and `freeze`'s plan rebuilt from sorted coordinates rather
        than from the tree. Kept, each faster with its output unchanged: `label` joining a
        leaf's voxels from its number (commit `fd3ad14`), `LeafManifest::build` through
        the accessor's path (commit `cc7aa15`), `freeze`'s plan from the tiles' numbers
        and runs of keys (commit `ad2bff5`), all 2026-10-25; walking the tree, not
        allocating, took the time. Labels stored as `u32` over the union-find's parents,
        chosen by the human, cut labelling's memory (2026-10-26; commit `8504ddb`)
- [x] B-016: a sign that does not depend on orientation, by a spike on synthetic broken
      meshes (`spikes/sign/`), as an option of conversion and Hermite data: the parity
      count along the axes, `Sign::Parity` (ADR-0027), faster than the winding number
      (2026-10-26; measured in commit `3f6e6d0`)
- [x] colino's layer 3, places thinner or gaps narrower than two voxels (B-020)
  - [x] Finding them, by rays along each sampled point's normal (ADR-0028; its cost
        measured in commit `56d0008`, 2026-10-26)
  - [x] Checking them by a local reconversion at h/2 (ADR-0028): the three invariants of
        each place's box at h and at h/2, a box beyond 2²⁶ voxels at h/2 unchecked
- [x] Retrospective and refactoring; benchmarks; mutation run (ADR-0023). Chosen by the
      human for this content (ADR-0033): no fuzz round or Miri run, unless a step adds a
      reader of untrusted bytes or unsafe code
  - [x] Retrospective, its proposals decided by the human (2026-10-26); refactoring:
        colino's vector helpers in `distance.rs`, the tree's box in `Bvh::bounds`
  - [x] Benchmarks within their baselines, both crates, at loads of 1.7 and 1.5
        (2026-10-26)
  - [x] A review of the logic and algorithms of both crates, chosen by the human
        (2026-10-26), what P5 wrote or rewrote first; each defect fixed on its own, with
        its test. Three passes, by module, by concern and by workflow, found 53 items
        (2026-10-26): readers of untrusted bytes that hang or exhaust memory on small
        files (3MF, cache, Hermite, OBJ), parts sharing a face split in two, a Hermite set
        written beside its SDF claiming the SDF's convention, and costs beyond the band.
        Fixed or decided by the human (2026-10-27), M2 set aside as B-023
  - [x] A review again of what the fixes changed, chosen by the human (2026-10-26): three
        read-only passes (2026-10-27), on brodo, colino and the documents, found a point on
        the mesh splitting overlapping parts, solids lost from one-line and CR-only STL,
        erosion endless on far leaves, writes at once sharing a hidden file and 13
        documents behind the code; each fixed or decided
  - [x] R12, a frozen view thawed into a topology and a field (ADR-0042), scheduled by
        the human (2026-10-26), after the review's fixes to the Hermite reader: landed
        (2026-10-27), `GridView::thaw`, `thaw_on` and `same_topology`
  - [x] Refactoring across the crates, its candidates listed for the human to choose:
        the conversion's lattice read once, the tests' boxes and clock shared, the view's
        root lookup shared (2026-10-27)
  - [x] The public API tidied in 0.6.0, already breaking for `Stored`:
        `FreezeError::Inconsistent` removed among it (2026-10-27)
  - [x] Benchmarks again at the close (2026-10-28): colino's within their baselines;
        brodo's `cache_write_x8` and `gather` behind, the sync of review G1 and a change
        of code placement, recorded again by the owner's decision (ADR-0019)
  - [x] Mutation run, after the review and the refactoring: each missed mutant of the
        runs on `9940bd5` and `fa72208` killed by a test or excluded under the contract
        (ADR-0023, ADR-0043); the run on `90342e7` missed none, its two lasting timeouts
        since killed (2026-10-28; commit `fc10a04`)
  - [x] Tests and exclusions held to the contract, not the implementation (ADR-0043),
        chosen by the human after tests written for mutants pinned more than the code
        promises (2026-10-28): T12, a test asserts a promise; T11's exclusions name where
        their difference is left open, checked by `mutants.py`
  - [x] A review of every promise against ARCHITECTURE.md §1, chosen by the human
        (2026-10-28): three read-only passes, on brodo, colino and the formats with how
        promises are governed, found promises in conflict with the code, behaviour users
        and tests rely on that nothing promised, and a format version whose meaning had
        changed; a promise given three homes (AGENTS.md), each finding written in its
        home or decided (ADR-0043, ADR-0038, ADR-0036)
- [x] **Accepted when** a badly oriented mesh converts with the new sign, layer 3 finds
      thin walls and narrow gaps, and each cost above is measured, kept where it helps:
      `sign_reference.rs`, `thin_reference.rs` and the costs' measurements; accepted
      2026-10-28

### P6 — brodo: archives and quantisation

Split from P5 by the human on 2026-10-25, and OpenVDB files from it into P7 on 2026-10-28,
so that the archive, within brodo alone, is not held by `vdb-rs`'s checks. Each step is a
spike and an ADR before it is built.

- [x] B-005: the archive's compressor, pure Rust, its dependency approved: `lz4_flex`
      (ADR-0016, 2026-10-29)
- [x] The archive profile (ARCHITECTURE.md §4.7), its reader in `UNTRUSTED_INPUT` and fuzzed
      (2026-10-29)
- [x] Value quantisation in the archive, 16-bit float and 8-bit range, with byte shuffling
      (ADR-0016, 2026-10-29)
- [x] Reading a region: only the chunks a box of voxels needs decompressed, each into a
      buffer of its own, read as a view that answers within the box and refuses the rest
      (the human, 2026-10-29; spikes/region/, ADR-0016)
- [x] Retrospective and refactoring (ADR-0001, ADR-0019, 2026-10-30)
- [x] Fuzz round, the new reader included: seven targets, 900 s each, no finding
      (2026-10-30)
- [x] Miri run: every `miri_*` test passes; Miri's nightly of 2026-07-21 still warns of
      integer-to-pointer casts in `crossbeam-epoch` 0.9.21 under Tree Borrows (ADR-0015,
      2026-10-30)
- [x] Benchmarks, every feature, the archive's with baselines, `cache_map_x256` and
      `cache_write_x8` looked into (ADR-0019, 2026-10-30); the whole read's buffer kept
      written (ADR-0016)
- [x] Mutation run over the workspace (ADR-0023), as the human chose (ADR-0033): each
      missed mutant killed or excluded as equivalent; the four files changed run again,
      none missed (2026-10-31)
- [x] **Accepted when** archives round-trip within their stated error:
      `archive_format.rs`, exact to the bit, quantised within docs/formats.md §8.5's
      bounds, a region as the whole grid within its box; accepted 2026-10-31

### Between P6 and P7

- [x] A spike on why parallel benchmark items vary from one process to the next: the
      leaf-parallel update and colino's `convert_slab_320`, each thread's leaves and time
      in a slow and a fast process (§1, B-024; the human, 2026-10-30)

### P7 — brodo: OpenVDB files

Split from P6 by the human on 2026-10-28. Each step is a spike and an ADR before it is
built. brodo reads and writes `.vdb` itself, the format learnt from OpenVDB at `v14.0.0`
(ADR-0044, ADR-0045; the owner, 2026-10-31).

- [x] OpenVDB from its Apache-2.0 release 12.0 read for the format (ADR-0044)
- [x] B-006: `vdb-rs` not used (ADR-0045)
- [x] Writing: float and double grids on brodo's tree, checked by OpenVDB's own tools
      (ADR-0046)
- [x] Reading: the files brodo writes, and those OpenVDB 12 to 14 writes for the same grid
      types by default (ADR-0047, 2026-11-01)
- [x] Retrospective and refactoring: a reading spike measures what its reader refuses or
      skips (ADR-0001), Rule 8's grep takes `String` only as `Result`'s error (ADR-0002),
      and the source guard ends a path at a shell's punctuation (ADR-0044); the magic
      number and active-mask flag shared by the writer and the reader (2026-11-01)
- [x] Fuzz round, the new reader included: eight targets, 900 s each, no finding
      (2026-11-01)
- [x] Miri run: every `miri_*` test passes; the same warning from `crossbeam-epoch` 0.9.21
      under Tree Borrows (ADR-0015, 2026-11-01)
- [x] Benchmarks, every feature, at rest: every item within its baseline but
      `leaf_parallel_update_x32`, as slow before P7, recorded again (ADR-0019, 2026-11-01)
- [x] Mutation run (ADR-0023): each mutant the run on `a7022af` missed in the `.vdb`
      reader and writer killed by a test or excluded; the two files run again on
      `b0a924c`, none missed (2026-11-01)
- [x] **Accepted when** brodo's float and double grids are written to `.vdb` files that
      OpenVDB's tools read back the same, and such files are read back into brodo:
      `vdb_format.rs`'s golden file, read back by OpenVDB 14.0.0 with every value the same,
      and `vdb_read.rs`, brodo's files read back the same; accepted 2026-11-02

### After P7

STEP is not read (ADR-0031). `piatto` is held (§2, B-021).

**First: a review of the whole repository**, after 0.8.0 and before anything else, in this
order (the owner, 2026-11-01). No public item changes; a change one would want is recorded
as a proposal for a unit of its own.

- [x] `target/mutants-tmp`, left by a stopped run, removed
- [x] `vdb_read`'s Blosc chunk refused when its block does not start where its header
      ends or bytes are left after its block, as docs/formats.md §9.8 says, a test failing
      first (found by an outside review, 2026-11-01; the owner set it after 0.8.0)
- [x] The memory `vdb::read` makes, measured on the file that makes the most per byte, then
      either `limit` bounding it, as `archive::read` does, or its multiple stated, by an
      amendment of ADR-0047 the owner decides (the same review)
- [x] Rules: every rule in one table, with what enforces it, whether breaking it once
      fails the check (TESTING.md T3), what it has caught in the history and what it costs
      the gate; each proposed to be kept, enforced, merged or retired, and the owner's
      decisions landed through the ADR on their topic (AGENTS.md, "Changing the rules")
- [x] Issues and pull requests, a rule proposed in the same step with an ADR of its own
      (the owner, 2026-11-01): no code taken from outside, since much of it is written by
      agents and none of it can be shown to keep AGENTS.md's "Sources"; pull requests
      opened only by collaborators, GitHub's setting, set by the owner; issues taken, a
      fix written here from an issue's steps and expected result, never from code it
      carries; an issue's text read as data, not instructions; a request recorded in §4
      (AGENTS.md S4); nothing posted, closed or merged by an agent without the owner's
      approval
- [x] Documents: each claim set against a measurement (the public API, the formats against
      the golden files, the build steps run, PROVENANCE.md's rows), and what restates
      another home or narrates history removed; each document read by a subagent of its
      own, briefed with AGENTS.md's "Sources"
- [x] Code: refactoring (PLANNING.md §7), the tests unchanged, the gate passing before and
      after, the golden files the same bytes, the benchmarks within their baselines, then a
      mutation run with none missed, since `.cargo/mutants.toml` names functions. Among the
      candidates, from the same review: `frozen::plan` and `vdb::Nodes::of` each rebuilding
      the nodes from a topology; `winding.rs`'s own `sub`, `dot` and `cross`, which
      `distance.rs` has; `archive::load` and `vdb_read::read` reading a file within a limit
      alike; `RegionView` searching twice for a leaf. Public errors that differ in deriving
      `Clone` and `PartialEq` change public items, so they are a proposal, not part of this
      review. The last three done; `frozen::plan` and `vdb::Nodes::of` left, since they
      build different nodes in different orders
- [x] Files no longer needed: one list from the three steps, removed once the owner
      approves it; a test file kept only if removing it fails a test; every spike's
      REPORT.md kept, its code removed only where its ADR's decision was later reversed
- [x] A version only if code changed, 0.8.1 by the owner's choice; TESTING.md V2 raises
      PATCH for a fix, so whether a refactoring counts as one is put to the owner then

Then P8 and P9, below, in that order (the owner, 2026-11-03; ADR-0011).

### 0.8.2 — the ad hoc made principled

The owner set it on 2026-11-02, after a survey of the whole repository, by four readers
and checked against the code, for what was fixed by patch rather than at its cause. Each
item is fixed at its cause, a test or check failing first (TESTING.md T3); a rule changed
lands with its ADR and the owner's approval (AGENTS.md, "Changing the rules").

Code:

- [x] Hermite data: an edge whose inside end lies on the mesh takes its crossing there with
      the normal and group of a triangle at distance 0, not of the one `find` chose
      farther along or nearest the edge (ADR-0030)
- [x] `distance::on_triangle`: edges divided by their largest component, so that nothing
      under- or overflows; turning the largest angle first was tried and taken back, its
      cost beyond what it bought (ADR-0027); the benchmarks within their baselines
- [x] The slice's zero contour unbroken where a leaf meets a tile, as ARCHITECTURE.md §6.1
      states (ADR-0018)
- [x] The BVH's pruning margins derived once from the rounding of `crossing` and
      `on_triangle`, in one helper, and `Bvh::nearest` held to its promise of the lowest of
      equals as `nearest_segment` is (ADR-0030)
- [x] A field's topology checked for every function that takes both, by a test calling each
      on an empty topology and on another, `stale_field.rs`, which a new such function
      joins; each function's own check kept, a structure for all of them changing public
      items
- [x] One definition of a triangle with no area, a unit normal and an area in colino;
      duplicated helpers (`on_segment` and `nearest_fraction`, `norm` and `length`, the
      centroid, union-find) made one; the tree's shape and a voxel's place in its leaf taken
      from `tree.rs` and `field.rs` rather than restated; `tile_source` one rule at every
      level; one length rule for an empty cache file in the three readers; a fallback no
      input reaches said so where it stands, and a Blosc chunk of no bytes refused as
      malformed rather than as unsupported

Checks (each a rule, with its ADR):

- [x] `.claude/settings.json` running both guards on every tool, not a list of names, so
      that a tool added later, such as one running a shell command, is guarded (ADR-0003)
- [x] The guards made a convenience, and the backstop made whole: the pushed range of
      commits checked in CI, and the unpushed here, by `commit_msg_check.py`, and every
      message by the project's rules; `claude_guard.py` splitting words as the shell does
      (`shlex`), quotes and escapes removed (ADR-0003, ADR-0034)
- [x] `.cargo/mutants.toml` read by its whole shape, its only key `exclude_re`, so that a
      quoted key is refused too (ADR-0023)
- [x] betterleaks reading every text file git would commit, its default allow-list's paths
      (`node_modules/`, `.gltf`, `.svg` and more) included, by `leak_input.py`, which the
      hook and the audit share (ADR-0003)
- [x] `source.py`'s Rust read by a lexer rather than regular expressions, with no
      dependency, so that an attribute holding two exceptions, a `src/target/` directory
      and Rule 8's other spellings are read (ADR-0002); Rule 9 by `flatten`, which needs
      types, left to review
- [x] `tests.py` listing doctests, a `compile_fail` one among them (ADR-0021)
- [x] One list of the files git would commit and of which are text, one heading parser and
      one list of the rule files, each imported where it is used now restated (ADR-0001)

Tests:

- [x] The tests of a triangle on a line: exactly collinear corners never crossed; whatever
      is crossed has area, as a property rather than a filter; and the Hermite one made a
      property asserting the bytes of the cube without a triangle with no area
- [x] The tests of one reported input (a far cube, a crossing at `1 + ε`) made properties
      over translation and scale
- [x] References that share no code with what they check (the edge distance, by the cross
      product)
- [x] The archive's fuzz target taking a region's box from its input, and every target the
      same probes
- [x] Each literal tolerance given its derivation (TESTING.md T4), the thin neck's and
      tube's thickness worked from their polygons
- [x] One random generator and one set of mesh and tree builders a crate, in a module of
      its own each, `tests/support/`, since a shared module whose every item a test does
      not use fails `dead_code`; proptest drawing in place of a hand-made sequence where it
      can

### P8 — Looking inside, and the formats settled

Set by the owner on 2026-11-03 (ADR-0011): the units left after P7's review, closed as
0.9.0. Each is started once the owner approves it; the estimates are an agent's working
time, the gate, the mutation run of what changed and the benchmarks besides (2026-11-02):

1. `limit` bounding what `vdb::from_bytes` and `vdb::read` make; first, since the command
   of 2 reads `.vdb` files as untrusted input and should read them within a limit (about
   half a day)
2. A command to look inside brodo's files (about a day, after its ADR)
3. README.md's "Looking inside" section (about an hour, from the viewers' documentation)
4. B-023 and B-022 together, since both raise a format version (about a day's spike, then
   two to three days)

- [x] `limit` bounding what `vdb::from_bytes` and `vdb::read` make, each leaf, tile and
      node counted as it is read at what it makes at most, since a cache file's bytes do
      not bound a tile's memory (the owner, 2026-11-03; ADR-0047); the fuzz target `vdb`
      passing a limit, as `archive`'s does
- [x] A command to look inside brodo's files, for someone who receives a cache file, an
      archive or a `.vdb` and writes no Rust; `piatto` stays held (B-021), so it uses no
      GPU and no GUI (the owner, 2026-11-02):
      - [x] An ADR, its options put to the owner with a recommendation each: `mestolo`, an
            application crate of its own, its arguments parsed with `std`, its output
            refusing a file already there but by `--force` (the owner, 2026-11-03; ADR-0050)
      - [x] `info`: each grid's name, value type, bounds, count of active voxels, least and
            greatest value, and metadata
      - [x] `slice`: an axis and a position, the slice written as PNG (`slice-png`)
      - [x] `convert`: a cache file or an archive to `.vdb` or VTK, and a `.vdb` too
      - [x] Tests on the golden files, each subcommand's output held to a committed one
            (`crates/mestolo/tests/commands.rs`, 2026-11-03)
- [x] README.md's "Looking inside" section: the command of the unit above, what brodo's
      checks have read back, and what Blender's, Houdini's and ParaView's documentation
      says they read, said apart from what was tried; the viewers not installed nor tried,
      the owner choosing on 2026-11-03 their documentation over installing them, as
      AGENTS.md's "Hierarchy of truth" allows for a claim outside the repository
- [x] B-023 (ADR-0051) and B-022 (ADR-0052), the owner's order (2026-11-02):
      - [x] B-023's spike: Hermite data's crossings held only on the edges that hold one,
            its memory and writing time measured against the dense fields' on spheres of
            radius 200, 600 and 1500 voxels (`spikes/hermitesparse/`, 2026-11-03): sparse,
            a mask a leaf and axis and the crossings packed, the signs a `u8` field, in an
            entry of named bytes the cache profile's version 2 gains (the owner,
            2026-11-03; ADR-0051)
      - [x] Version 2 of the frozen layout, the cache and archive profiles and Hermite data
            stated in docs/formats.md §2.5, §3, §4.1 and §8 before the code, B-022's
            checks all taken (the owner, 2026-11-03; ADR-0052)
      - [x] Hermite data's format version raised, its crossings held sparsely; the frozen
            layout's, the cache profile's and the archive profile's raised with B-022's
            checks; each reading the version before it and writing it on request
            (ADR-0038), `formats.py` reading version 2 and its golden files joining §7's
            (2026-11-03)
- [x] The closing checks the owner chooses by P8's content (ADR-0033), all five (the
      owner, 2026-11-03):
      - [x] Retrospective and refactoring: no duplication worth merging found; two
            proposals put to the owner, undecided: a reader check docs/formats.md gains
            held by `formats.py` to the golden files already there in the same commit,
            and TESTING.md T10's benchmarks checked mechanically (2026-11-04)
      - [x] Fuzz round, version 2's golden files among the seeds: eight targets, 900 s
            each, no finding (2026-11-03)
      - [x] Miri run: every `miri_*` test passes; the same warning from `crossbeam-epoch`
            0.9.21 under Tree Borrows (ADR-0015, 2026-11-03)
      - [x] Benchmarks, every feature: version 2's bounds check, over every place, made
            the cache map 28 times slower, and is now over the set places alone;
            `cache_map_x256`, still about twice as slow, recorded again (ADR-0019,
            2026-11-03); every item within its baseline after `ad37aab` (2026-11-04)
      - [x] Mutation run (ADR-0023): the run on `1599360` (GitHub Actions run
            41713041735) missed 9, and 3 timed out after the patient run's 1800 s; the
            walk the 3 made endless rewritten, 3 missed killed by tests (`ad37aab`), each
            mutant applied by hand, and 6 excluded (`caf012d`); not run again
            (2026-11-04)
- [x] Version 0.9.0 (TESTING.md V2–V4, 2026-11-04)
- [ ] **Accepted when** `vdb::read` refuses a file that would make more than `limit`
      bytes, the command's subcommands match their golden files, README.md's section shows
      only what was opened and seen, and the formats B-023 and B-022 raise read the
      version before them

### P9 — 1.0 on crates.io

Set by the owner on 2026-11-03 (ADR-0011): brodo and colino first published to crates.io,
as 1.0.0, and mestolo with them (the owner, 2026-11-04). Each step is started once the owner approves it; publishing cannot be undone,
since crates.io yanks a version but never deletes it.

- [x] B-018 and B-019 revisited, as their rows say (§3): neither taken now (the owner,
      2026-11-04)
- [x] The public API reviewed for what would break a user after 1.0: each public enum and
      error `#[non_exhaustive]` or not, each public trait sealed or not, the public errors'
      `Clone` and `PartialEq` (After P7's review), each change a proposal to the owner:
      `Connectivity` `#[non_exhaustive]`, `Ordered` sealed, `png`'s error kept out of
      `SliceError`, and zerocopy 0.8 stated a public dependency; the other enums, `Crossing`
      and `Betti` kept exhaustive, and no error given `Clone` or `PartialEq`, which can be
      added later (the owner, 2026-11-05)
- [ ] Designs grown by accretion surveyed before 1.0, each taken, in the order put to the
      owner (the owner, 2026-11-05):
  - [x] colino's `mmap` on brodo dropped; `LeafId` and `Probe` hidden; the writers' value
        traits made markers; `mestolo`'s library stated; one `Axis`; Hermite data's dense
        leftovers dropped
  - [x] version 1 of the frozen layout and the cache and archive profiles no longer read,
        and docs/formats.md and ARCHITECTURE.md stated at the present versions (ADR-0054)
  - [x] the readers' and writers' names and arguments made one shape, colino's mesh
        readers and mestolo's output with them (ADR-0055)
  - [x] a value type named by an enum, and a field of any stored type, a cache file's
        grid thawed as one within a limit and a file's kind told by brodo (ADR-0056)
  - [x] a cache file's grids reached as its entries of bytes are; the reserved-key writers
        hidden (ADR-0057)
  - [ ] the errors' shapes, places and features made even
- [x] One prefix of metadata keys reserved for the formats brodo builds on the cache
      profile after 1.0, such as `brodo.`, `hermite.` kept as it is, so that each later
      one is not a breaking change to `CacheEntry::new` and `CacheEntry::bytes`, which
      refuse only `hermite.` now (the owner, 2026-11-04; its ADR): `brodo.`, Hermite
      data's and the SDF convention's keys moved under it, their earlier versions not read
      (ADR-0053)
- [x] Patents and licences judged again by the owner, for registration on crates.io:
      unchanged (2026-11-04; ADR-0007)
- [ ] The rules for versions after 1.0, with their ADRs (AGENTS.md, "Changing the
      rules"): TESTING.md V1's `0.MINOR.PATCH` and V2's breaking MINOR, a breaking change
      raising MAJOR; V4 publishing each release to crates.io; ADR-0040's pinned tag;
      ARCHITECTURE.md §7's "Before 1.0"; `rust-version`'s rise, a MINOR or a MAJOR
- [ ] Packaging: `publish` set; each crate's keywords naming its function (sparse volume,
      VDB, SDF, level set, voxel) and its categories; colino's dependency on brodo by
      version as well as path; what each package holds, listed by `cargo package --list`;
      `cargo publish --dry-run` of each; docs.rs's features set; both names unregistered
      (checked 2026-10-16 and 2026-11-03)
- [ ] mestolo published as well, installed by `cargo install mestolo` (the owner,
      2026-11-04): its name checked unregistered, its dependency on brodo by version as
      well as path, and what its command line and output promise from 1.0 stated where
      they are held
- [x] The gate's tests run on macOS and Windows as well as Linux in CI (the owner,
      2026-11-04), the `mmap` feature, paths and `mestolo`'s refusal of a file already
      there among what Windows has not run; README.md states what brodo builds for, the
      64-bit, little-endian targets `lib.rs` and `frozen.rs` hold it to; the `platforms`
  job of `audit.yml` (ADR-0034), whose first run, on 18a7c3c, passed 407 tests on macOS
  and 404 on Windows, the three that need `mkfifo` not built there (2026-11-05)
- [ ] A way to report a vulnerability privately, decided by the owner before anything is
      published (2026-11-04), since brodo reads untrusted files (TESTING.md T6): a
      `SECURITY.md`, and GitHub's private vulnerability reporting, a setting the owner makes
- [x] Version 2's media types stated in docs/formats.md §3, §4.1 and §8, `version=2`
      beside version 1's, and ADR-0041 amended (the owner, 2026-11-04); §6's versions with
      them, version 2 written since 0.9.0
- [ ] A check that a release after 1.0 breaks no public API its version does not allow,
      asked by the owner (2026-11-04): cargo-semver-checks, which compares the API's
      rustdoc JSON with a baseline's, a published version or a tag, every feature on, is
      an agent's reading of its documentation (2026-11-04), measured by nothing; it misses
      a field's or argument's type changed, generics and lifetimes, and what only some
      features show, so review stays; its version is pinned to one that reads the pinned
      toolchain's rustdoc JSON; it reads a library, not `mestolo`'s command line
- [x] Golden files of version 2's bare frozen buffers, `f32` and three `f64` as version
      1's (docs/formats.md §7), held by the Format tests, so that the fuzz round's seeds
      of `grid_view` come from `tests/data/` as TESTING.md T6 says; P8's were made by hand
      from version 1's (the owner, 2026-11-04); `frozen_v2_f32.bin` and `frozen_v2_f64x3.bin`,
      written 2026-11-05
- [ ] The closing checks the owner chooses by P9's content (ADR-0033), the mutation run
      among them, of the code P8 checked only by mutants applied by hand (the owner,
      2026-11-04)
- [ ] 1.0 decided by the owner, in an ADR (TESTING.md V5); version 1.0.0 set and tagged,
      then brodo published, then colino, then mestolo, each with the owner's approval
- [ ] **Accepted when** the three crates are on crates.io at 1.0.0, docs.rs builds the
      libraries' reference, a new crate depending on them by version alone builds and runs
      each README's example, and `cargo install mestolo` installs the command

### Later

- [ ] Reading a region of an archive from only the bytes of the chunks it needs, the file
      not read whole (left to later by the human, 2026-10-29; ADR-0016)

## 3. Open questions and spikes

Each question is answered by a spike's measurement and an ADR, never in passing by an
implementation (PLANNING.md §2).

| # | Question | Proposal so far |
| :--- | :--- | :--- |

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
  owner on a preliminary search for this form of distribution (ADR-0007), and are asked
  again when the form changes, registering on crates.io among them.
- The API reference is not hosted: the crates are not on crates.io, so docs.rs holds
  none, and a reader builds it with `cargo doc` (README.md). Hosting it, on GitHub Pages
  from each release tag, is the owner's to decide.
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
