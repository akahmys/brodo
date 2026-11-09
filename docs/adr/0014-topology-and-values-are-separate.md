# ADR-0014: Topology and values are separate, and a topology edit returns a remap

- **Status**: Amended
- **Date**: 2026-10-16

Condensed on 2026-11-08 (ADR-0001): each decision is kept with its reason and what it set
aside; the text before is in git, this file as of `53f69d5`.

## Context

A solver holds many fields (density, velocity, temperature) over one active region.
Values stored in the leaves would duplicate the topology per field and scatter memory.
Changing a topology shared by several fields silently invalidates them all.

## Decision

The human decided, on 2026-10-16 unless dated otherwise.

- **Two types.** `Topology` holds the tree, the leaves and where tiles are; `Field<T>`
  holds one contiguous `Vec<T>` of leaf values (leaf count × 512), its tile values and its
  background. The tree is the only source of truth.
- **Numbering** (2026-10-17). A topology numbers its leaves and tiles in the Morton order
  of their origins and rebuilds the numbering at each commit, so that field values are in
  their final order from the first field on; it locates a voxel through the tree.
  `Topology::leaf_voxels(n)` gives a leaf's voxels in the order of its values, and
  `TileLevel::edge()` a tile's edge, after six files had computed both by hand; tests keep
  their own formulas, so that a reference stays independent of the code it checks.
- **The manifest** (2026-10-17), part of `Topology` and built at each commit, holds each
  leaf's 26 neighbours, each a `Location`: a leaf, a tile with its level, or the
  background, so that a gather fills from a tile without another lookup, rather than a
  number with `NONE` where absent, which CODING.md Rule 5 forbids; a row costs 208 bytes a
  leaf rather than 104.
- **Edits.** Topology changes go through an editor whose `commit` returns a `Remap` from
  old to new; every field on the topology must be remapped. Value updates and topology
  edits happen in separate phases. The arenas are rebuilt at each commit rather than kept
  with free lists, which would make every access fallible or panicking (Rule 2).
- **A stale field is refused by a stamp** (B-001, `spikes/remap/`). Every topology carries
  a stamp unique in the process, each field the stamp it was made or remapped for, and any
  use with another topology, a remap included, is `StaleField`. `commit` leaves the old
  topology usable, so that a refused remap leaves the caller its old topology and fields.
  A brand, an invariant lifetime, refused stale fields at compile time but confined each
  topology to a closure, a time loop of commits becoming recursion (Rule 15); a topology
  owning its fields needed workarounds and type erasure; none cost measurably more.
- **Sources** (B-008). `Remap` records where each new leaf's values come from (an old
  leaf, the tile it was split from, or the background) and each new tile's. A split leaf
  takes each field's own tile value. A region whose leaf the edit removed is taken as the
  background, whatever the edit puts there (2026-10-27: a leaf made again had taken the
  removed leaf's values).
- **Merges** (B-010, `spikes/merge/`, one level above leaves). A merged tile's sources are
  a list of parts, old leaves, old tiles and counts of empty regions, all merged tiles'
  in one list, each its range (a list per tile held 2.8 times more for small merges). The
  merge rule sees each value with the number of voxels holding it,
  `Fn(&[(T, u32)]) -> Option<T>`, a tile one entry, which made merging tiles 200 to 250
  times faster; `None` refuses the merge, and a rule refusing one tile refuses the commit.
  brodo gives the rule accepting only equal bits, through a `Value` trait; any other is
  the caller's. A merge takes leaves and tiles at once, in one commit (tiles only took two
  commits for the same result). A region holding nothing refuses the merge, so that a
  merge changes values, not which voxels are active. Merging into a root tile is left out
  until asked: listing its parts by leaf-sized region would visit about 1.3 × 10⁸ regions.
- **`Grid<T>`** (2026-10-17) binds one topology and one field for single-field use, built
  with P1's slice so that it checks a shape kernel's use as well as a solver's. It holds
  its merge rule, chosen when it is made. Its `edit` takes a closure, since an editor
  borrows the topology, and commits and remaps in one step, the grid unchanged on any
  error. It writes its field itself (`par_update_leaves`, `leaf_mut`, `set_tile`) and hands
  out none mutably (2026-10-27: a field of another topology put through `parts_mut` left
  the grid stale for good).

## Consequences

Every field on a topology is remapped by its holder after each commit; a field forgotten
fails as stale at its next use, not silently.

## Amendment

- **Date**: 2026-11-10

Issue 7 asked for a field moved onto an unrelated topology, voxels absent from the source
taking the background. The owner decided, on 2026-11-09 and 2026-11-10, as proposed:

- A `Remap` is made between any two topologies by `Remap::between(&from, &to)`, not by a
  commit alone, and the values are moved by `Field::remap`, so that one mechanism carries a
  field across an edit and onto another topology. Its sources are those a commit records,
  found as a commit finds them with nothing removed: `LeafSource`, `TileSource` and `Part`
  are not `#[non_exhaustive]`, and a variant added would break a user.
- A new tile covered whole by an old tile of its size or larger takes that tile's value,
  without the merge rule, as a tile split by a commit does. One over mixed content is
  merged by the caller's rule, its empty regions among its parts as `Part::Background`.
- A new root tile over anything but one old root tile or nothing is refused as
  `TreeError::MergeUnsupported`, as a merge into a root tile is: listing its parts by
  leaf-sized region would visit about 1.3 × 10⁸ regions.
