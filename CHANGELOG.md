# Changelog

Each version, newest first, with the milestones it closed and every change that breaks a
user (TESTING.md V2, V3). Format versions of the `GridView` layout and the file profiles
are recorded here when they change (V6).

## Unreleased

- `brodo::tree`: the sparse tree, with activation, tiles and their split; `brodo::error`.
- `brodo::topology` and `brodo::field`: topologies numbered in Morton order, their editor and
  `Remap`, and fields that refuse a stale topology. `Tree::leaves` and `Tree::tiles`.
- `brodo::manifest`: each leaf's 26 neighbours, built with its topology at each commit.
- `brodo::parallel` and `brodo::gather`: the leaf-parallel update, the fixed-order reduction,
  and the gather of a leaf with one ghost layer.
- `brodo::grid`: `Grid<T>`, one topology and one field, edited and remapped in one step.
- Merging: `Tree::merge` and `TopologyEditor::merge` into lower- and upper-level tiles,
  `TileSource::Merged` with `Remap::parts`, `brodo::field::Value`, `MergeRule` and
  `uniform`; `Grid::with_rule`. Commits rebuild the tree's arenas.

- `brodo::accessor`: `ValueAccessor` and `ValueAccessorMut`, reading and writing by voxel
  from the regions last visited; writes go to leaves only (`FieldError::NotALeaf`).

- `brodo::frozen` and `brodo::view`: `freeze` packs a topology and a field into one 32-byte
  aligned buffer, read in place by `GridView` after it checks the buffer whole.
- Frozen layout version 1 (TESTING.md V6); readers accept version 1 only (ADR-0016).
- `brodo::meta` and `brodo::cache`: `GridMeta` and `Bounds`, and the cache profile's files
  of grids with their metadata, written by `cache::write` and read and checked whole by
  `CacheFile`.
- Cache format version 1 (TESTING.md V6); readers accept version 1 only (ADR-0016).
- `CacheFile::map`: a cache file read by memory mapping, without a copy; the caller keeps
  the file unchanged while it is mapped (ADR-0008).

### Breaking

- `Field::remap` takes a merge rule: `remap(&remap, &uniform)` keeps the earlier behaviour.
- Field values are bounded by `brodo::field::Value` instead of `Copy + Send + Sync`; `Value`
  now requires `zerocopy`'s `FromBytes`, `IntoBytes`, `Immutable` and `KnownLayout`.

## 0.1.0 (2026-10-16)

Closes P0: the repository with its rules, the `brodo` and `colino` crates (empty), and the
design. No API and no format yet.
