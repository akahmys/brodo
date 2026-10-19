# Changelog

Each version, newest first, with the milestones it closed and every change that breaks a
user (TESTING.md V2, V3). Format versions of the `GridView` layout and the file profiles
are recorded here when they change (V6).

## Unreleased

- `brodo::slice::render` refuses a colour map whose range is not finite or is empty, with
  the new `SliceError::BadColorMap`; it drew such a range in no colour of its own. A
  `match` over `SliceError` needs the new arm.
- The cache reader refuses a file with bytes its writer never puts: a gap, non-zero
  padding or unused words, anything after the last grid, a metadata key twice or out of
  order. Files brodo wrote still read; cache format version stays 1.
- `brodo::euler::euler`: the Euler characteristic of the voxels meeting a condition, under
  6- or 26-connectivity (ADR-0024), with the new `AnalysisError::Unbounded`. A `match`
  over `AnalysisError` needs the new arm.

## 0.2.0 (2026-10-17)

Closes P1: `brodo`, a sparse volume with fields sharing a topology, leaf-parallel work,
a frozen view and cache files read in place, topology analysis, and checking output.

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

- `brodo::analysis`: connected-component labelling (`label`) and flood fill (`flood_fill`)
  on a condition on values, with 6- or 26-connectivity, tiles and the background
  included, each component saying whether it reaches the outside.

- `brodo::slice` (feature `slice-png`, on by default): a slice of a scalar field drawn as an
  image, with a diverging or sequential colour map over a range the caller gives, and
  encoded as PNG.
- `Topology::leaf_voxels`: a leaf's voxel coordinates in the order of its values;
  `TileLevel::edge`: the edge, in voxels, of a tile's region.
- `brodo::vtk` (feature `vtk`, on by default): `write_vtu` writes a scalar field as one
  ASCII VTK UnstructuredGrid of voxel cells, for ParaView; infinities and names needing
  XML escapes are refused (`VtkError`).

### Breaking

- `Field::remap` takes a merge rule: `remap(&remap, &uniform)` keeps the earlier behaviour.
- Field values are bounded by `brodo::field::Value` instead of `Copy + Send + Sync`; `Value`
  now requires `zerocopy`'s `FromBytes`, `IntoBytes`, `Immutable` and `KnownLayout`.

## 0.1.0 (2026-10-16)

Closes P0: the repository with its rules, the `brodo` and `colino` crates (empty), and the
design. No API and no format yet.
