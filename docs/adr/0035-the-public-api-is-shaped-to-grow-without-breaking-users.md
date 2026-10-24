# ADR-0035: The public API is shaped to grow without breaking users

- **Status**: Accepted
- **Date**: 2026-10-25

## Context

P4 reviews what brodo and colino show before P5 adds to colino's options and report
(ROADMAP.md §2). The API as it stood (2026-10-25):

- Of 36 public enums, 28 in brodo and 8 in colino, one, `brodo::gather::GhostWidth`, was
  `#[non_exhaustive]`. The other 35 include every error enum, 11 in brodo and 5 in colino,
  so each variant added since P1 broke every `match` over it, as CHANGELOG.md recorded
  again and again.
- 14 structs had public fields: brodo's `GridMeta`, `Slice`, `Image`, `Crossings`,
  `CrossingsView`; colino's `Options`, `Limits`, `Mesh`, `Converted`, `Produced`, `Report`,
  `Region`, `Betti`, `Surface`. A field added to any of them broke code that builds it
  field by field; `Mesh` gaining `groups` in P3 was one such break.
- colino's `bvh`, `distance`, `intersect` and `winding` were public modules, the reference
  tests outside the crate using them.

Measured with two throwaway crates on rustc 1.98.1: from another crate, a
`#[non_exhaustive]` struct cannot be built by a struct expression, even with
`..Default::default()` (E0639), while its public fields can still be read and assigned; a
`#[non_exhaustive]` enum cannot be matched without a wildcard arm (E0004).

## Decision

The owner chose, on 2026-10-25, the recommended option of each point:

- **a. Error enums.** (1) All `#[non_exhaustive]`. (2) As now. *Recommended: (1)*: errors
  grow with every reader and check, and a caller matching them needs a wildcard arm anyway
  to be safe.
- **b. Other enums.** (1) `#[non_exhaustive]` only where variants may be added:
  `Accuracy`, `ColorMap`, `Reference`, `Place`, `Rule`, `Label`; closed sets stay
  exhaustive, so that users' matches over them stay checked: `TileLevel` (the three tile
  levels of the fixed four-level tree, ADR-0013), `Location`, `ViewLocation`,
  `Connectivity`, `Axis`, `Bounds`, `Probe`, `Activation`, `Deactivation`, `NodeKind`,
  `LeafSource`, `TileSource`, `Part`. (2) All of them. *Recommended: (1)*.
- **c. Structs a user reads but never builds** (`Converted`, `Produced`, `Report`,
  `Region`, `Surface`, `Image`): (1) `#[non_exhaustive]`, fields public to read. (2) As
  now. *Recommended: (1)*.
- **d. Structs a user builds** (`Options`, `Limits`, `Mesh`, `GridMeta`, `Slice`):
  (1) `#[non_exhaustive]`, each with a constructor taking what it cannot do without
  (`Options::new(h)`, `Limits::new(triangles, vertices, bytes)`, `Mesh::new(vertices,
  triangles)`, `GridMeta::new(…)`, `Slice::new(…)`), the rest set by assigning its public
  field. (2) Fields private, with a builder. (3) As now. *Recommended: (1)*: it keeps the
  fields readable and assignable, as measured, and costs one constructor each.
- **e. Structs fixed by a format or by mathematics** (`Crossings`, `CrossingsView`, the
  three fields of Hermite data's version 1; `Betti`, three numbers): (1) left exhaustive.
  (2) `#[non_exhaustive]` too. *Recommended: (1)*: a new field there is a new format
  version or a new number, a break by nature.
- **f. colino's building blocks** (`bvh`, `distance`, `intersect`, `winding`): (1) public
  and kept to the same care as the rest. (2) Private to the crate, their reference tests
  moved inside it as unit tests (T2's renames recorded here). (3) Public but
  `#[doc(hidden)]`, promised to no one. *Recommended: (1)*: they parse nothing, so they add
  no attack surface, and a producer of SDFs or Hermite data other than colino may use them;
  `Accuracy` must stay public in any case, being in `Options`.

## Consequences

Every change is breaking once, listed together in CHANGELOG.md for 0.5.0. A test outside
the crates, built against the API as a user would build it, shows that a variant or a
field added later does not break it (P4's acceptance).

## Amendment

- **Date**: 2026-10-25

Building the decision corrected two points of the proposal, within the rule the owner
chose:

- `Label` was listed as an enum that may grow, but a voxel is either in a component or
  excluded, so it stays exhaustive; colino's report matches it whole.
- Expressions that built a struct inline, such as a conversion's options for one call,
  became blocks under assignment alone. `Options`, `Mesh` and `Limits` gained `with_…`
  methods returning a copy with one field changed (`Options::with_origin`, `with_band`,
  `with_accuracy`, `with_unit`; `Mesh::with_unit`, `with_triangles`, `with_groups`;
  `Limits::with_triangles`, `with_vertices`, `with_bytes`), beside their constructors and
  public fields. `Slice::new` and `Image::new` were added for the tests that build them.

## Amendment: the acceptance test

- **Date**: 2026-10-25

`api_growth.rs` in each crate's tests, compiled as a user's crate is, holds P4's
acceptance. Each enum that may grow is matched by naming every variant and a wildcard arm;
each struct a user builds is built by its constructor and set by assignment, and each one a
user reads is read by its fields. Measured: without `#[non_exhaustive]` on `brodo`'s
`Rule` or `colino`'s `Place`, the gate's clippy refused the wildcard as an unreachable
pattern; with a variant added to `MetaError` and a field to `Options`, both tests built and
passed unchanged. With that variant unnamed, clippy's `wildcard_enum_match_arm`, one of the
workspace's lints and not a user's, refused the brodo test, so a variant added later is
added to the test's list too, which keeps the first check true.

