# Architecture decision records

Why a rule or the design is the way it is. The design itself is in
[ARCHITECTURE.md](../../ARCHITECTURE.md); what needs a record is PLANNING.md §2.

## Writing one

- **One topic per record.** Before adding a record, check whether an existing one on the
  topic can be amended: add a dated `## Amendment` section and set its Status to
  `Amended`. A new record is for a new topic, or for reversing an old one.
- **Past tense**, under a page. If it needs more, the design belongs in ARCHITECTURE.md.
- A record is not deleted. A reversed one is marked `Superseded by ADR-NNNN`.

```
# ADR-NNNN: <the decision, as a statement>

- **Status**: Proposed | Accepted | Amended | Superseded by ADR-NNNN
- **Date**: YYYY-MM-DD

## Context
## Decision
## Consequences
```

A record is `Proposed` until the human accepts it.

## Index

| # | Decision |
| :--- | :--- |
| [0001](0001-the-rules-are-split-by-question.md) | The rules and the design are split by the question each document answers |
| [0002](0002-code-rules-held-by-the-compiler-and-clippy.md) | Code rules are held by the compiler and clippy, and Power of 10 is adopted in part |
| [0003](0003-commits-are-scanned-and-the-hooks-guarded.md) | Commits are scanned for secrets, personal data and outside paths, and the hooks are guarded |
| [0004](0004-a-retrospective-and-refactoring-close-every-milestone.md) | A retrospective and refactoring close every milestone |
| [0005](0005-dependencies-are-pure-rust-and-permissively-licensed.md) | Dependencies are pure Rust, permissively licensed, and declared once |
| [0006](0006-untrusted-input-is-fuzzed-outside-the-workspace-and-the-gate.md) | Untrusted input is fuzzed, outside the workspace and the gate |
| [0007](0007-copyleft-sources-are-never-read.md) | Copyleft and sibling sources are never read, and every source file records what it drew on |
| [0008](0008-unsafe-is-confined-to-the-memory-mapping-module.md) | Unsafe code is confined to one module, brodo's memory mapping, and run under Miri |
| [0009](0009-indexing-is-banned-in-parsing-modules.md) | Indexing is banned, and fuzzing required, per parsing module rather than per crate |
| [0010](0010-the-licence-is-mit-or-apache-2.md) | brodo is licensed MIT OR Apache-2.0 |
| [0011](0011-milestones-follow-the-project-group-and-versions-are-in-the-changelog.md) | Milestones are the project group's phases, and versions are recorded in CHANGELOG.md |
| [0012](0012-brodo-is-a-standalone-sparse-volume-base.md) | brodo is a standalone sparse-volume base, in two crates |
| [0013](0013-the-tree-has-four-levels-with-tiles-in-arenas.md) | The tree has four fixed levels with tiles, nodes in arenas, and a BTreeMap root |
| [0014](0014-topology-and-values-are-separate.md) | Topology and values are separate, and a topology edit returns a remap |
| [0015](0015-parallel-updates-and-gather-work-per-leaf.md) | Parallel updates and stencil gathers work per leaf, with a fixed-order mode |
| [0016](0016-a-frozen-view-and-two-file-profiles.md) | A frozen grid is one aligned byte buffer, stored by two file profiles |
| [0017](0017-colino-reports-topology-changes-and-does-not-repair.md) | colino converts meshes to narrow-band SDFs and reports topology changes without repairing them |
| [0018](0018-visualisation-is-checking-output-in-brodo-and-a-viewer-in-piatto.md) | Visualisation is checking output in brodo and a viewer in a third crate, `piatto` |
| [0019](0019-benchmarks-run-on-a-std-only-harness-of-brodos-own.md) | Benchmarks run on a std-only harness of brodo's own, and fail 10 % behind their baseline |
| [0020](0020-tests-are-shown-to-reach-and-catch-what-they-guard.md) | Tests are shown to reach the cases they are meant for, and to fail where they guard |
| [0021](0021-the-gate-covers-every-target-feature-set-and-way-to-ignore-a-test.md) | The gate covers every target, every feature set, and every way to ignore a test |
| [0022](0022-every-test-is-listed-so-a-deleted-test-needs-an-adr.md) | Every test is listed, so that a deleted test needs an ADR |
| [0023](0023-mutation-testing-closes-a-milestone-behind-a-memory-guard.md) | Mutation testing closes a milestone, behind a memory guard |
| [0024](0024-the-euler-characteristic-counts-the-cells-each-lattice-vertex-owns.md) | The Euler characteristic counts the cells each lattice vertex owns |
| [0025](0025-dilation-and-erosion-work-on-whole-leaves-without-a-mask.md) | Dilation and erosion work on whole leaves, without a per-voxel mask |
| [0026](0026-colino-reads-meshes-as-given-under-limits.md) | colino reads meshes as given, under limits, and welds by exact position |
| [0027](0027-colino-converts-by-exact-distance-and-fast-winding-numbers.md) | colino converts by exact distance and fast winding numbers, into a band |
| [0028](0028-the-topology-report-counts-betti-numbers-and-region-connectivity.md) | The topology report counts Betti numbers and region connectivity |
| [0029](0029-f64-vectors-are-a-value-type.md) | `[f64; 3]` is a value type |
| [0030](0030-colino-keeps-triangle-groups-and-gives-hermite-data-on-voxel-corners.md) | colino keeps triangle groups and gives Hermite data on voxel corners |
| [0031](0031-colino-reads-triangle-meshes-not-step.md) | colino reads triangle meshes, not STEP |
| [0032](0032-hermite-data-is-a-brodo-format-any-producer-writes.md) | Hermite data is a brodo format that any producer writes |
| [0033](0033-a-milestones-closing-checks-are-chosen-by-its-content-after-p4.md) | A milestone's closing checks are chosen by its content after P4 |
