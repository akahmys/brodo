# Changelog

Each version, newest first, with the milestones it closed and every change that breaks a
user (TESTING.md V2, V3). Format versions of the `GridView` layout and the file profiles
are recorded here when they change (V6).

## Unreleased

- colino converts, and gives Hermite data, by a sign that does not depend on the
  triangles' orientation as an option: `Options::with_sign(Sign::Parity)`, a parity count
  along the three axes; the winding number stays the default (`Sign::Winding`). `Options`
  and `Converted` gain a field `sign`, and Hermite data follows its conversion's
  (ADR-0027).
- Labelling holds 4 bytes a voxel for its labels rather than 8 beside the union-find's
  parents; a component number reaching `u32::MAX` is refused as too large.
- Faster, with the same output: Hermite data's unhit edges, labelling, `commit`'s leaf
  manifest and `freeze`; Hermite data holds no work field over every leaf (ROADMAP.md §2,
  P5's costs).

## 0.5.0 (2026-10-25)

Closes P4: the public API shaped to grow without breaking users, what a user does not need
neither built nor reached, and the file formats specified in one document, held by a reader
written from it alone.

- `docs/formats.md` states the bytes of the frozen layout, the cache profile and Hermite
  data, the SDF convention, the media type names `application/vnd.brodo.cache` and
  `application/vnd.brodo.hermite`, and the version policy; where it and the code differ,
  the document is right (ADR-0037, ADR-0038, ADR-0041).
- colino's SDFs carry `sdf.version` = `1` in `GridMeta::extra`, naming brodo's SDF
  convention (`brodo::meta::SDF_KEY`, ADR-0039); a cache file written from one gains the
  pair.
- `CacheFile::map` is an `unsafe fn`: its caller promises, in its own `unsafe` block, that
  the file is not changed while mapped (ADR-0036).
- `CacheFile::read` takes a byte limit and refuses a longer file by its length before
  reading it, with the new `CacheError::FileTooLarge` (ADR-0036).
- brodo's features `slice-png` and `vtk` are off by default: a user drawing slices or
  writing VTK names them (ADR-0036). Mapping a cache file is behind the feature `mmap`, on
  by default.
- colino's readers are features, `stl`, `obj` and `threemf`, all on by default; with
  `default-features = false`, a user names those it needs (ADR-0036).
- Error enums, and enums that may gain variants (`Accuracy`, `ColorMap`, `Reference`,
  `Place`, `Rule`), are `#[non_exhaustive]`: a `match` over one outside its crate needs a
  wildcard arm (ADR-0035).
- Structs a user only reads (`Converted`, `Produced`, `Report`, `Region`, `Surface`,
  `Image`) and structs a user builds (`Options`, `Limits`, `Mesh`, `GridMeta`, `Slice`) are
  `#[non_exhaustive]`: built through `Options::new`, `Limits::new`, `Mesh::new`,
  `GridMeta::new`, `Slice::new` and `Image::new`, then set by a public field or a `with_…`
  method; a struct expression no longer builds them outside their crate (ADR-0035).

## 0.4.0 (2026-10-25)

Closes P3: a group per triangle read from OBJ, STL and 3MF; Hermite data as a brodo format
any producer writes, with its check; and colino producing it from a mesh, crossings found
by a watertight segment-triangle test.

- brodo has a README, the front page of its documentation, its example a doctest.
- `colino::mesh::Mesh` gains `groups` and `group_names`, a group per triangle, and
  `check_groups` and `group`; `welded` keeps each triangle's group and refuses groups that
  do not fit (ADR-0030). A `Mesh` written field by field needs the two fields, or
  `..Mesh::default()`. `MeshError` gains `GroupCount`, `UnnamedGroup` and
  `GroupZeroNamed`: a `match` over it needs the new arms.
- `colino::obj::read` reads `g` and `o` as groups: the last `g`'s names, else the last
  `o`'s (ADR-0030).
- `colino::stl::read` reads each ASCII solid as a group, named by its words; binary STL
  has none (ADR-0030).
- `colino::threemf::read` reads each object holding a mesh as a group, named by its `name`,
  else its `id` as written; every placement shares it (ADR-0030).
- `brodo::hermite::{Hermite, Crossings, HermiteView, CrossingsView}`: Hermite data as a
  format any producer writes, checked whole when made and read, written to a cache file as
  ten grids told apart by `hermite.` keys in `GridMeta::extra`, format version 1
  (ADR-0032); `brodo::error::{HermiteError, Rule}`.
- `colino::intersect::{crossing, nearest}`: where a segment crosses a triangle, by Woop et
  al.'s watertight test in `f64` with ties decided exactly, and the point of a segment
  nearest a triangle; `Bvh::near_segment` and `Bvh::corners` (ADR-0030).
- `colino::hermite::{hermite, Produced}` and `colino::error::HermiteError`: a mesh's Hermite
  data on its conversion's topology, as `brodo::hermite`'s set, with the count of crossed
  edges no triangle was hit on (ADR-0030); `Bvh::len` and `Bvh::is_empty`. A badly oriented
  mesh whose winding number changes sign beyond the band is refused with
  `HermiteError::SignOutsideBand`.

## 0.3.0 (2026-10-22)

Closes P2: `colino`, triangle meshes read from STL, OBJ and 3MF into a narrow-band signed
distance field by the generalised winding number, and a report of the solid's topology;
in `brodo`, the Euler characteristic, dilation and erosion.

- colino has a README, the front page of its documentation, and two examples: a torus
  checked against its exact distance, and a vented box whose vent closes at a coarser
  voxel size.
- `brodo::gather::{gather_wide, GhostWidth}`: a leaf and one, two or three ghost layers
  copied into a caller's `Vec` (ADR-0015). `GhostWidth` is `#[non_exhaustive]`: a `match`
  over it needs a wildcard arm.
- `brodo::vtk::VtkScalar` is renamed `VtkValue` and gains `COMPONENTS`; `write_vtu` also
  writes `[f32; 3]` and `[f64; 3]` fields, as three-component `Vectors` (ADR-0018). Code
  naming `VtkScalar` needs the new name; a scalar field's bytes are as before.
- `[f64; 3]` is a value type, type code 6, frozen and cached under layout and format
  version 1 (ADR-0029). brodo 0.2.0 refuses a grid of it, naming the code; files of every
  other type keep their bytes.
- `brodo::slice::render` refuses a colour map whose range is not finite or is empty, with
  the new `SliceError::BadColorMap`; it drew such a range in no colour of its own. A
  `match` over `SliceError` needs the new arm.
- The cache reader refuses a file with bytes its writer never puts: a gap, non-zero
  padding or unused words, anything after the last grid, a metadata key twice or out of
  order. Files brodo wrote still read; cache format version stays 1.
- `brodo::euler::euler`: the Euler characteristic of the voxels meeting a condition, under
  6- or 26-connectivity (ADR-0024), with the new `AnalysisError::Unbounded`. A `match`
  over `AnalysisError` needs the new arm.
- `Tree::deactivate` and `TopologyEditor::deactivate` remove a leaf, and the nodes it
  leaves empty, returning a `Deactivation`; inside a tile they refuse with the new
  `TreeError::InTile` (ADR-0025). A `match` over `TreeError` needs the new arm.
- `brodo::morphology::{dilate, erode}`: a topology dilated or eroded by k steps, 6- or
  26-connected, on whole leaves, with its `Remap` (ADR-0025).
- `brodo::morphology::{dilate_values, erode_values}`: a max or min over each leaf voxel's
  6 or 26 neighbours, k times, for the scalar types (`Ordered`), a NaN winning; tiles and
  the background unchanged.
- `colino::mesh::{Mesh, Limits}` with `Mesh::welded`, and `colino::stl::read` for binary
  and ASCII STL (ADR-0026); `colino::error::{ReadError, MeshError}`.
- `colino::obj::read` for Wavefront OBJ: vertices and faces, polygons as fans (ADR-0026).
- `colino::threemf::read` for 3MF packages: every build item placed (ADR-0026).
  `Limits` gains `bytes`, which every reader checks: a `Limits` written field by field
  needs it.
- `colino::distance` and `colino::bvh::Bvh`: the nearest point of a triangle, and the
  nearest triangle of a mesh, the lowest-numbered of equals (ADR-0027).
- `colino::winding::{Winding, Accuracy, solid_angle}`: the generalised winding number,
  exact or by Barill et al.'s approximation (ADR-0027).
- `colino::convert::{convert, Options, Converted}` and `colino::error::ConvertError`: a
  mesh into a narrow-band signed distance field with inside tiles and its metadata
  (ADR-0027).
- `colino::surface::Surface`: a mesh's vertices, edges and faces, and why it is not a
  closed 2-manifold if it is not (ADR-0028).
- `colino::report::{report, Report, Betti, Reference, Region, Place, Pair}` and
  `colino::error::ReportError`: the topology report's first two layers, the solid's and
  the complement's Betti numbers, χ against the input's when it is a closed 2-manifold,
  and the components of named points (ADR-0028).
- colino's feature `serde`, off by default: the report and `Surface` derive `Serialize` and
  `Deserialize` (ADR-0028).

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
