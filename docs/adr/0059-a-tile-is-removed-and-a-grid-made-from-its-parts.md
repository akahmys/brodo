# ADR-0059: A tile is removed in one call, and a grid is made from its parts

- **Status**: Accepted
- **Date**: 2026-11-05

## Context

TESTING.md T14's survey before 1.0 (ROADMAP.md P9) found, by running brodo:

- No tile could be removed. `deactivate` refuses a voxel in a tile (ADR-0025), so emptying
  one took a `split_tile` and a `deactivate` per leaf-sized region: eroding a single root
  tile by one voxel took 3.5 s and left 22 086 000 tiles (2026-11-05).
- A `Grid<T>` could not be made from a topology and a field, nor take a remap it had not
  committed itself, so a grid thawed from a file, a dilation or erosion, and the values
  `dilate_values` gives could not become or change a grid.
- `LeafManifest` derived a public `Default`, a manifest belonging to no topology.
- `Grid::edit`'s closure returns a `TreeError`, so a caller's own fallible work cannot use
  `?` inside it.

## Decision

The owner decided, on 2026-11-05, as proposed:

- `TopologyEditor::remove_tile(v)` removes the tile holding `v`, at any level, root tiles
  included, and the nodes it leaves empty, giving the tile's level; a voxel in no tile is
  refused with `NotATile`. As for a removed leaf, a leaf or tile the same edit puts in the
  region takes nothing of the tile's value: the commit gives it the background.
  `InTile`'s message names removing beside splitting.
- `Grid::from_parts(topology, field, rule)` refuses a field of another topology with
  `StaleField`; `into_parts` gives the three back; `Grid::replace(topology, &remap)` remaps
  the field by a remap from its topology to `topology`, and leaves the grid unchanged if
  the remap is from another topology or to another one, or the rule refuses a merge.
- `LeafManifest` has no `Default`.

On `Grid::edit`, the proposal was a closure returning the caller's error `E`, with
`E: From<GridError>`. Measured while implementing: a closure returning a `TreeError`
directly, `g.edit(|e| e.add_tile(level, v))`, no longer compiles, `TreeError` not being
`From<GridError>`, and a closure using `?` on one needs its error type written; the form
every caller in the repository uses breaks. Put back to the owner, who decided, on
2026-11-05: `edit` is unchanged, a shorthand for the steps it joins, and edits failing
with the caller's own error are made by those steps, the topology's copy edited,
committed and given to `replace`, the grid unchanged if any fails. `edit`'s documentation
shows them, a doctest the gate runs; no second way of editing is added.

## Consequences

A region is emptied of a tile in one call, and a grid is built from whatever gives a
topology and a field on it. The tree's and the remap's reference tests hold `remove_tile`
to their models, each reaching a removed root (the tree's), upper and lower tile and a
refused one. `LeafManifest::default()` no longer compiles. `edit`'s signature is unchanged. The bytes are unchanged.
