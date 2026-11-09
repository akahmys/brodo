# Changelog

Each version, newest first, with the milestones it closed and every change that breaks a
user (TESTING.md V2, V3). Format versions of the `GridView` layout and the file profiles
are recorded here when they change (V6).

## Unreleased

- `GridView::regions` and `ViewRegions`: a view's leaves' origins, by leaf number, and its
  tiles' levels, origins and values, by their place among the tile values, read without
  thawing (ADR-0042).
- `CacheError::BytesDecompress` and `CacheError::BytesChunks`, with the `archive` feature:
  an archive's entry of bytes whose chunk does not decompress, or that is not cut as the
  format cuts it, named by its place among the entries of bytes (ADR-0057).
- Fixed: `CacheError::Grid`, `Decompress` and `Chunks` name a grid by its place among the
  file's grids, as `GridEntry::index` numbers it, from a whole read as from a region's,
  where a whole read counted entries of bytes before it; 1.0.0's note that the index is
  the entry's place in the directory was superseded by ADR-0057 (issue 14).
- Fixed: `HermiteView::find` refuses a set as of an earlier version, by its key
  `hermite.set`, only on an entry holding no `brodo.hermite.set`; a set of this version
  carrying `hermite.set` as a free key no longer makes another set of that name refused
  (docs/formats.md §4.2, rule 1; issue 15).

## 1.1.0 (2026-11-09)

Closes P10, rounding off 1.0: what brodo, colino and mestolo lacked against their
promises, added without breaking a user (TESTING.md T14, V2). No format version changes.

- `Field::set_background`; `LeafNumber::new`, `TileNumber::new`, `Offset::from_index` and
  `Offset::from_delta`; `Field<T>` taken from an `AnyField` by `TryFrom` (ADR-0062).
- `GridEntry::encoding` and `Region::encoding`, an archive grid's value encoding, with the
  `archive` feature (ADR-0062).
- mestolo's `--version` and `-V`, `help <command>` and a command's `-h` or `--help`;
  `convert --skip-unsupported`; `Command::HelpOn`, `Command::Version` and
  `Output::skip_unsupported` (ADR-0050).
- colino's `Limits` implements `Default`: 2²⁶ triangles and vertices, 4 GiB (ADR-0026).

## 1.0.0 (2026-11-08)

Closes P9, the first release on crates.io (ADR-0061): the formats settled, the public API
reviewed and reshaped, the rules for versions after 1.0, and the packages. From this
version the promises follow semantic versioning (TESTING.md V2).

Format versions (TESTING.md V6): Hermite data is at version 3 and the SDF convention at
version 2, their keys moved under `brodo.` (ADR-0053); the frozen layout and the cache and
archive profiles stay at version 2, and version 1 of them is no longer read nor written
(ADR-0054). No earlier version of any is read, the owner setting V6's rule aside before
1.0. Every change below breaks a user.

- `cache::to_bytes_version`, `cache::write_version`, `archive::to_bytes_version`,
  `archive::write_version` and `FrozenGrid::with_version` are gone, with
  `FreezeError::Version`, `CacheError::BytesAtVersion1` and `FormatError::Shared`; a file
  or buffer of version 1 is refused with `UnsupportedVersion(1)`.

- Keys beginning `brodo.` are reserved: `CacheEntry::new` refuses one but
  `brodo.sdf.version`, and `CacheEntry::bytes` and `Hermite::entries` every one, with
  `ReservedKey`; `CacheEntry::with_reserved_keys` and
  `CacheEntry::bytes_with_reserved_keys` still write one. Keys beginning `hermite.` are
  free.
- `SDF_KEY` is `brodo.sdf.version` and `SDF_VERSION` is `2`; a grid holding `sdf.version`
  is no longer taken for an SDF, by `mestolo slice` or by the `.vdb` writer's `class`.
- A Hermite set's keys are `brodo.hermite.version` (`3`), `brodo.hermite.set`,
  `brodo.hermite.field` and `brodo.hermite.group.<id>`. A set of version 1 or 2 is
  refused by `HermiteView::find` with `UnsupportedVersion`. `Hermite::entries_v1`, `FIELDS`,
  `VERSION_1`, `HermiteView::version` and `HermiteError::OtherMeta` are gone.
- `analysis::Connectivity` is `#[non_exhaustive]`: a `match` on it ends in a wildcard arm.
- `morphology::Ordered` is sealed: brodo's scalar value types are its only ones.
- `SliceError::Encode` holds a `std::io::Error`, the PNG encoder's error inside it, in
  place of `png::EncodingError`, so that the encoder's crate is not a public dependency.
  zerocopy 0.8 is one: `field::Value`'s bounds are its traits.
- colino no longer turns on brodo's `mmap`, which it does not use: a program that maps
  cache files and reached `mmap` through colino names it on brodo.
- `tree::LeafId` and `tree::Probe` are no longer public, and `Activation::Existing`,
  `Activation::Created` and `Deactivation::Removed` carry no id: a leaf's id meant nothing
  once the edit was committed, which numbers leaves as `LeafNumber`.
- `vdb::VdbValue` and `vtk::VtkValue` are markers: their constants and methods, what
  writing a value needs, are brodo's own.
- `mestolo`'s library promises `run`, `USAGE`, `args` and `error`; its other modules are
  hidden and promise nothing (ADR-0050).
- `tree::Axis`, always built, is the axis of index space: `slice::Axis` is it, and
  `Crossings::get`, `HermiteView::crossing` and `HermiteError::Violation` take or hold it
  in place of a `usize`.
- `hermite::NO_CROSSING`, `hermite::Expanded` and `Hermite::expand`, a dense view of a set
  held sparsely, are gone. An edge whose ends' signs differ and that holds no crossing is
  refused for `Rule::NoCrossing`, not `Rule::Fraction`.
- Every format reads and writes in one shape, `from_bytes`, `read`, `to_bytes` and
  `write` (ADR-0055): `CacheFile::read` and `CacheFile::from_bytes` are `cache::read` and
  `cache::from_bytes`; `archive::read_region` and `archive::region_from_bytes` take the
  limit last, after the grid's index and the extent; `vtk::write_vtu`, which wrote to a
  `Write`, is `vtk::to_bytes(t, f, name)` and `vtk::write(path, t, f, name)`;
  `slice::encode_png` is `slice::to_bytes`, beside the new `slice::write`, and
  `SliceError` gains `Io`. A `.vtu` and a PNG written to a path are written whole, as a
  cache file is.
- colino's `stl::read`, `obj::read` and `threemf::read`, which took bytes, are
  `from_bytes`; `read` now takes a path, refusing a file longer than `Limits::bytes`
  before reading it. `ReadError` gains `Io` and no longer derives `Clone`, `PartialEq` or
  `Eq`, nor is `UnwindSafe` or `RefUnwindSafe`, since the `std::io::Error` it may hold is
  neither.
- A stored value type is named by `field::ValueType` (ADR-0056): `Stored::TYPE_CODE` is
  `Stored::VALUE_TYPE`; a grid's and a region's `value_type` return a
  `ValueType`; `FormatError::WrongValueType` holds two, and a header of the type asked for
  but another size is `Inconsistent("header")`; `CacheError::NotQuantised` holds an
  `Option<ValueType>`, `None` for an entry of bytes.
- `vdb::VdbField` is gone: `VdbGrid::field` is a `field::AnyField`, which holds a field of
  any stored type. `GridEntry::thaw` and `GridEntry::thaw_size` thaw a grid of the type its
  file states within a limit, refused as `CacheError::ThawTooLarge`. `kind::FileKind::of`
  tells a file's kind by its first bytes.
- mestolo's `MestoloError::ValueType`, `NotScalar` and `NotWritten` name a `ValueType`.
- A cache file's grids are reached as its entries of bytes are (ADR-0057):
  `CacheFile::grids` gives a `GridEntry` for each, with `index`, `meta`, `bounds`,
  `value_type`, `view`, `thaw_size` and `thaw`; `CacheFile::grid_count`, `meta`, `bounds`,
  `value_type` and `view`, which took a grid's number, are gone. `grids().len()` counts
  them.
- `CacheEntry::with_reserved_keys` and `CacheEntry::bytes_with_reserved_keys` are
  `#[doc(hidden)]`: they stay for brodo's own formats and for tests, and promise nothing.
- Errors are of one shape, in one place (ADR-0058): `HermiteError::Version` and
  `VdbError::Version` are `UnsupportedVersion`, and every version error says "version N is
  not read by this build"; `GridError` and `AnalysisError` are `brodo::error::GridError`
  and `brodo::error::AnalysisError`; `brodo::euler::euler` is `brodo::analysis::euler`.
- `SliceError`, `VdbError` and `VtkError` are built only with `slice-png`, `vdb` and
  `vtk`, and `CacheError`'s `Decompress`, `Chunks`, `UnknownEncoding`, `NotQuantised` and
  `Unrepresentable` only with `archive`.
- `manifest::LeafManifest` no longer implements `Default`: one belonging to no topology
  meant nothing (ADR-0059). A topology's is `Topology::manifest`.
- `vdb::from_bytes` and `vdb::read` read a float grid of `class` `level set` meeting
  ARCHITECTURE.md §4.7's conditions as stating the SDF convention: its free pairs hold
  `brodo.sdf.version` = `2` in place of `class`, an SDF brodo wrote among them (ADR-0047,
  amended). Code that took such a grid for a level set by `class` finds `SDF_KEY`.
- `mestolo` run with no argument exits 2, the usage on standard error: `args::parse` of
  none is `MestoloError::NoCommand`, where it was `Command::Help`. `mestolo help` still
  writes the usage and exits 0 (ADR-0050, amended).
- `mestolo slice` refuses a plane beyond the grid along its axis, as
  `MestoloError::OutsideGrid`, where it drew the background.
- `MestoloError::NotWritten` holds every grid `.vdb` does not hold, each its name and
  value type, where it held the first.
- `Command::Convert` has `format`, the format `--as` names: `mestolo convert --as cache`
  and `--as archive` write a cache file or an archive, a cache file's or an archive's
  entries copied as they were.

A file of version 1, written by 0.8 or earlier or by 0.9.0 on request, is first read by
0.9.0 and written again by it, at version 2. A file of version 2 then converts with this
release alone, its grids and entries of bytes still read as a cache file
(`GridEntry::view`, `GridView::thaw`, `CacheFile::bytes_entries`), and written again with
each key renamed:

- a Hermite set of version 2: the sign grid and the entry of crossings, each `hermite.*`
  key renamed `brodo.hermite.*` and `brodo.hermite.version` set to `3`, written with
  the hidden `CacheEntry::with_reserved_keys` and `CacheEntry::bytes_with_reserved_keys`;
- a Hermite set of version 1, ten dense grids in a cache file 0.9.0 wrote again: its grids
  thawed, every leaf value other than fraction −1, normal zero and group 0 given to
  `Crossings::builder` leaf by leaf, the set made by `Hermite::new` with the sign grid and
  the `hermite.group.<id>` names, and written by `Hermite::entries`;
- an SDF: `sdf.version` renamed `brodo.sdf.version`, its value `2`, the grid written again.

The three crates are set to be published to crates.io (TESTING.md V8, ADR-0040): each
package holds its sources, examples, README and both licences, its tests and benchmarks
left out. Nothing a user builds against changes. A vulnerability is reported privately,
as SECURITY.md says.

## 0.9.0 (2026-11-04)

Closes P8: `mestolo`, a command that looks inside brodo's files without Rust (`info`,
`slice`, `convert`; ADR-0050); a limit on what reading a `.vdb` makes (ADR-0047); and
version 2 of the frozen layout, the cache and archive profiles and Hermite data, its
crossings held sparsely (ADR-0051, ADR-0052). Every change below breaks a user.

Format versions (TESTING.md V6): the frozen layout, the cache profile, the archive profile
and Hermite data are each at version 2, written; version 1 of each is read, and written on
request. A reader of an earlier release refuses a file of version 2 (docs/formats.md §2.5,
§3, §4.1, §8; ADR-0051, ADR-0052).

- `freeze`, `cache::to_bytes`, `cache::write`, `archive::to_bytes` and the archive's other
  writers write version 2. Version 1 is written by `FrozenGrid::with_version(1)`,
  `cache::to_bytes_version`, `cache::write_version`, `archive::to_bytes_version` and
  `archive::write_version`.
- A version 2 reader refuses what version 1 took: nonzero padding or background bytes
  beyond the value's size, a gap between sections, nodes out of Morton order, a node with
  no child or tile, a first child or tile other than the count before it, and a cache
  file's bounds other than its grid's.
- `Hermite` holds its crossings sparsely. `Crossings` is now that sparse type, built by
  `Crossings::builder`, its three dense fields gone; `Hermite::new` takes it in place of
  the three axes; `Hermite::axes` and `HermiteView::axes` are gone, and `CrossingsView`
  with them, for `Hermite::crossings`, `Hermite::expand` and `HermiteView::crossing`.
  `Hermite::entries` writes version 2, a sign grid and an entry of bytes;
  `Hermite::entries_v1` writes the ten grids of version 1. `hermite::VERSION` is `"2"`.
- `CacheError::Grid`'s and `CacheError::Decompress`'s `index` is the entry's place in the
  directory, entries of bytes counted.

- `vdb::from_bytes` takes a `limit`, as `archive::from_bytes` does: a caller passes a
  third argument. With `vdb::read`'s, it bounds what reading makes, counted as
  ARCHITECTURE.md §4.7 states, and more is refused as `VdbError::FileTooLarge`; a file
  `vdb::read` took at a limit of its length may now need a larger one (ADR-0047).

## 0.8.2 (2026-11-02)

Closes no milestone: what a survey of the whole repository found fixed by patch rather
than at its cause, fixed at its cause (ROADMAP.md §2, "0.8.2"). One change breaks a user,
the first below; the owner kept the version a PATCH all the same, an exception to
TESTING.md V2.

- `CacheFile::map` refuses a file of no bytes as `CacheError::Truncated("header")`, as
  `CacheFile::read` and `from_bytes` do; 0.8.1 gave `Inconsistent("length")`. A caller
  matching the old variant for an empty file breaks.
- `vdb::from_bytes` and `vdb::read` refuse a Blosc chunk stating no bytes as
  `VdbError::Malformed("Blosc chunk")`; 0.8.1 gave it as unsupported.
- `brodo::slice::render` draws the zero contour on a tile's voxel too, where it is nearer
  zero than a leaf's voxel of the other sign beside it, so that the line does not break
  at a leaf's edge (ARCHITECTURE.md §6.1, ADR-0018). An image with such a meeting changes.
- `colino::distance::on_triangle` gives the nearest point whatever the triangle's size,
  10⁻¹⁰⁰ to 10¹⁰⁰ tested, where 0.8.1 found a point above a face of a triangle 10⁻¹⁰⁰ wide
  on an edge (ADR-0027).
- Hermite data: an edge whose inside end lies on the mesh takes the normal and group of
  the triangle holding that end, not of one farther along (ADR-0030).
- `colino::bvh::Bvh::nearest` keeps its promise of a scan's answer with a margin derived
  from the rounding of a distance, as the segment queries' margins now are (ADR-0030).
- The repository's checks: both guards on every tool, the guard reading the shell as the
  shell splits it, and each commit not yet pushed, or pushed, held to the commit-msg check
  in the audit (ADR-0003, ADR-0034); every text file passed to betterleaks whatever its
  name (ADR-0003); Rust read by a lexer, every exception's part held, and Rule 8 refusing
  any untyped error (ADR-0002); doctests listed (ADR-0021); `.cargo/mutants.toml` read by
  its shape (ADR-0023); one list of the files, the headings and the rule files (ADR-0001).
- The READMEs, rewritten for a user: what the crates are for, where to start, their
  features, how the code is written, by its owner with AI coding agents, and why pull
  requests are not accepted.

## 0.8.1 (2026-11-02)

Closes no milestone: the review of the whole repository after P7. Nothing breaks a user.

- `vdb::from_bytes` and `vdb::read` refuse a Blosc chunk whose block does not start at its
  byte 20, after its start entry, or with bytes after its values, as docs/formats.md §9.8
  states; 0.8.0 read past them.
- The documentation of `vdb::from_bytes` states how much memory reading makes, and that of
  `vdb::read` that its `limit` bounds the file, not what reading makes (ARCHITECTURE.md
  §4.7, ADR-0047).
- A topology edit that removed a leaf and merged its lower node's region into an upper-level
  tile, where an old tile held the region, gave the new tile the old tile's value, or the
  background, without the merge rule; the removed leaf's region now counts as the
  background, as ARCHITECTURE.md §4.2 states (ADR-0014).
- `archive::read_region` and `region_from_bytes` refuse the leaf chunks beyond `limit`
  before making room for them; 0.8.0 made it first, a multiple of `limit`. They
  refuse an upper node, lower node or leaf the walk of the box reaches twice as
  `FormatError::Shared`, a new rule of docs/formats.md §8.6; 0.8.0 walked a shared node
  again for each entry naming it.
- `morphology::dilate_values` and `erode_values` refuse a field of another topology when
  `k` is 0, and `analysis::Flood::contains` when the seed met no condition; 0.8.0 answered.
- `colino::distance::on_triangle` gave, for a triangle whose corners lie nearly on a line,
  a point farther than its nearest edge's, so a distance too large, by up to 0.13 of the
  triangle's size; the distance is now within the rounding of the coordinates over the sine
  of the triangle's sharpest angle, at most √ε of its size (ARCHITECTURE.md §5.1, ADR-0027).
- `colino::intersect::crossing`, and so Hermite data and the parity count, crossed a
  triangle whose corners lie on a line where a segment passed through the line, against
  its documentation; Hermite data was then refused for a normal of no length. A triangle
  with edges below about 10⁻⁸¹ or above about 10⁷⁷ got such a normal too (ADR-0030).
- `colino::report::report` refuses a region whose point is not finite, as the new
  `ReportError::Region`; 0.8.0 placed it outside the grid (ADR-0028).
- The README shows the logo, kept with the icons in `docs/logo/`.
- `scripts/audit/formats.py`, the reader written from docs/formats.md, reads an archive's
  region as §8.6 says, compared with the crates in the gate's tests (ADR-0037).
- The documentation of `colino::error::ReportError::Mesh` names groups that do not fit the
  triangles, and ARCHITECTURE.md §6.1 the zero contour drawn two pixels wide where values of
  equal size and opposite sign meet.
- The repository's guards and checks read what they missed: commands that got round the
  commit hooks, terminal and browser tools, sources named after `:` or `=`, Rules 8, 9 and
  19 and T1 written over several lines or under `cfg_attr`, output directories deeper than
  the root, a mutation run whose baseline failed, was missing or whose list was empty, and
  a commit message's ADR not staged or named only in `git commit -v`'s diff, strings over
  lines, and SVGs and a README's HTML links (ADR-0002,
  ADR-0003, ADR-0007, ADR-0021, ADR-0023, ADR-0034).

## 0.8.0 (2026-11-02)

Closes P7: OpenVDB's `.vdb` files, brodo's float and double grids written as OpenVDB reads
them back, and read from the files brodo and OpenVDB 12 to 14 write by default.

- OpenVDB files, under the new feature `vdb`: `vdb::VdbEntry::new` takes an `f32` or `f64`
  grid with its metadata, and `vdb::to_bytes` and `vdb::write` give one `.vdb` file at file
  version 225, which OpenVDB 14.0.0 reads back with every value the same, as
  docs/formats.md §9 states (ADR-0046). The new `VdbError` refuses two grids of one name.
- Reading OpenVDB files: `vdb::from_bytes` and `vdb::read` read the float and double grids
  of a `.vdb` file at file version 224 or 225, uncompressed, with active-mask or with Blosc's
  LZ4, as brodo and OpenVDB 12 to 14 write them by default, into a `VdbFile` of `VdbGrid`s,
  each with its `GridMeta`, `Topology` and a `VdbField` of `f32` or `f64`, and the names of
  the grids skipped: of another type, saved as half floats, or instanced. Inactive leaf
  voxels keep their values; inactive tiles holding the background are left out. Zip, other
  maps and other Blosc codecs are refused, as docs/formats.md §9.7 states (ADR-0047). The
  feature `vdb` depends on `lz4_flex`.

## 0.7.0 (2026-10-31)

Closes P6: brodo's archive profile, its grids compressed by `lz4_flex` in chunks of 512
leaves, their floating values exact, in binary16 or in 8 bits, read back within their
stated bounds, whole or one grid's region at a time.

- The archive profile, under the new feature `archive`: `archive::write` and `to_bytes`
  store grids with their metadata, each frozen buffer in chunks of 512 leaves compressed
  by `lz4_flex`'s block format, and `archive::read` and `from_bytes` read one back as the
  `CacheFile` of the same grids, checked whole first, as docs/formats.md §8.4 states
  (ADR-0016). Archive format version 1. `CacheError` gains `Decompress` and `Chunks`;
  `BadMagic` and `UnsupportedVersion` now name no profile in their messages.
- Quantised values in the archive: `archive::write_with` and `to_bytes_with` take each
  grid's `archive::Encoding`, exact, binary16 byte-shuffled, or 8 bits over its range, for
  floating value types, its leaf values read back within docs/formats.md §8.5's bounds
  (ADR-0016). Archive format version 1 gains the value encoding code; exact archives keep
  their bytes. `CacheError` gains `UnknownEncoding`, `NotQuantised` and `Unrepresentable`.
- Reading a region of an archive: `archive::read_region` and `region_from_bytes` read one
  grid's box of voxels, decompressing only its first chunk, its tile chunk and the leaf
  chunks the box needs, as an `archive::Region` whose `RegionView` answers within the box;
  a chunk not decompressed is not checked (docs/formats.md §8.6, ADR-0016). `FormatError`
  gains `OutsideRegion`.

## 0.6.0 (2026-10-28)

Closes P5: colino's sign without orientation, by parity, and its topology report's third
layer, the places thinner or gaps narrower than two voxels; the costs a review found,
each measured and kept where it helped; and the promises users rely on, each in its home
and held by tests to it, not to the code.

- `brodo::tree::Tree` is no longer public: a topology is built and read through
  `Topology` and `TopologyEditor`, and the tree's own leaf numbers and orders bound the
  code to no user (ADR-0036). How colino's `bvh`, `distance`, `intersect` and `winding`
  work, as their modules tell it, is not promised; what their functions give is.
- A grid whose type code no value type has is refused as the new
  `FormatError::UnknownValueType`, not as `WrongValueType { expected: 0, .. }`; code
  matching that needs the new variant. docs/formats.md §6 now says what may change within
  a format version, and §2.6 what a reader rebuilding the tree also refuses (ADR-0038).
- A cache file whose bounds' least corner passes its greatest along an axis is refused,
  `CacheError::Inconsistent("bounds")` (docs/formats.md §3.4, rule 4). This refuses files
  version 1 accepted; brodo writes none.
- colino's topology report has its third layer: `Report::thin`, the places thinner, or
  gaps narrower, than two voxels, each a `Thin` with its kind (`ThinKind::Wall` or `Gap`),
  box, least thickness or width and count of points, found from the mesh along its
  triangles' normals, and checked by its box's `Invariants` at h and at h/2 (`Thin::check`,
  a `Check`: `Same`, `Changed` or `Unchecked`) (ADR-0028). A mesh lying farther than 2³¹
  voxels from the world's origin is refused, the new `ReportError::TooFar`.
  `Report::located` is now true.
  colino depends on `rayon` outside its tests, for the layer's points.
- colino converts, and gives Hermite data, by a sign that does not depend on the
  triangles' orientation as an option: `Options::with_sign(Sign::Parity)`, a parity count
  along the three axes; the winding number stays the default (`Sign::Winding`). `Options`
  and `Converted` gain a field `sign`, and Hermite data follows its conversion's
  (ADR-0027).
- Labelling holds 4 bytes a voxel for its labels rather than 8 beside the union-find's
  parents; a component number reaching `u32::MAX` is refused as too large.
- A frozen view thaws into an owned topology and field, `GridView::thaw`, or into a field
  on a topology holding its leaves and tiles, `GridView::thaw_on`, refusing another with
  the new `FormatError::OtherTopology`; `GridView::same_topology` is public. Thawing walks
  the view's nodes once, so its cost follows the buffer, and refuses a view whose nodes,
  leaves or tile values are reached twice, `FormatError::Shared`, or a leaf never reached,
  `FormatError::Inconsistent` (ADR-0042, R12).
- `euler` sums the tiles' edge vertices in parallel, keeping no set of them: on a sphere
  of radius 400 voxels converted by colino, 1.8 to 2.7 s against 4.5 to 5.5 s, and 532 MB
  at most against 857 MB, the old and new code run in turn on a loaded machine
  (2026-10-27).
- Less memory, with the same output (2026-10-27, old and new run in turn): colino's
  conversion frees each step's topology and field as the next is made, 103 to 105 MB at
  most against 142 MB on a sphere of radius 200 voxels; `cache::write` writes the file part
  by part and `CacheFile::read` reads it straight into aligned blocks, 888 MB against
  1.30 GB to write and 412 MB against 822 MB to read a file of 410 MB.
- Faster, with the same output: Hermite data's unhit edges, labelling, `commit`'s leaf
  manifest and `freeze`; Hermite data holds no work field over every leaf (ROADMAP.md §2,
  P5's costs).

### Fixed

- colino's 3MF reader refuses a component naming the object it belongs to, as naming no
  object defined before it (`ReadError::NoObject`); such a file made the reader run for
  ever, or until memory ran out.
- `CacheFile::read` reads no more than its limit and one byte: a device, a pipe or a file
  growing after its length was read had passed the length's check and been read whole,
  `/dev/zero` until memory ran out.
- The Hermite check takes a point beyond the `i32` range as the background, as the rest of
  brodo and docs/formats.md §4.2 do, rather than the range's other end: a set at both ends
  of a range was judged by voxels 2³² apart.
- The Hermite check of a tile's faces costs what the nodes beside them hold, not the faces'
  area: a root tile's faces had been checked 8 × 8 patch by patch, 1.5 million points, so
  that a small file of root tiles asked for much work.
- colino's OBJ and 3MF readers look up a group's name once, not once a face or a placement:
  a long name over many faces took time following their product.
- colino's OBJ reader takes its statements one at a time: it held them all, about 49 bytes
  for each byte read, 3.3 GB at most for 64 MB of empty lines against 69 MB now
  (2026-10-27, `/usr/bin/time -l`, release).
- colino's third layer leaves a place unchecked when its box at h/2 holds more voxels than
  an `i64` counts, rather than overflowing the count.
- colino's report refuses a transform not aligned with the axes, `ReportError::Transform`,
  whether or not a point is placed: the third layer read only its diagonal.
- A commit that removes a leaf and adds a root tile over it no longer fails a debug
  assertion; the tile holds the background, as in a release build.
- `slice::render` refuses a range wider or higher than a `u32` counts, or whose image does
  not fit in memory, with the new `SliceError::TooLarge`, rather than `EmptyRange` or a
  panic.
- colino's conversion refuses a voxel size below `f32`'s normal range, or a band's
  half-width beyond `f32`, `ConvertError::BadOption`: the field's distances could not hold
  them.
- A tree refuses a node or a leaf that would make its arena's count beyond a `u32`,
  `TreeError::ArenaFull`, so that numbering every leaf never overflows.
- `label` refuses a topology whose voxels and tiles a `u32` does not number before making
  any unit, `AnalysisError::TooLarge`, rather than overflowing the walk's unit numbers.
- colino's 3MF reader refuses a vertex number beyond a `u32`, rather than clamping it.
- colino's third layer starts each ray off its own triangle by 2⁻⁴⁰ of the coordinates
  it works with, rather than 10⁻⁹ of the mesh's size: far from the origin the start was
  lost to rounding and the ray crossed its own face, and on a large mesh it could pass
  over walls thinner than the start.
- colino refuses a mesh whose triangles reach a vertex that is not finite, the new
  `MeshError::NotFinite`: a mesh made by hand with a NaN converted without an error, and
  the BVH's sort lost its total order.
- `write_vtu` refuses a field of another topology when the topology is empty, as it does
  otherwise, `FieldError::StaleField`.
- colino's Hermite data from a conversion with a leaf at the end of the `i32` range takes
  the background beyond it, sign 0, rather than overflowing.
- Morphology ends for the largest k: value dilation and erosion stop once a pass changes
  nothing; erosion looks at a cell's surroundings nearest first, stopping at the first
  background cell, rather than listing every cell within reach, and removes no more for k
  beyond the box's widest side; and a dilation whose new leaves a `u32` could not number
  is refused before any is made, `TreeError::ArenaFull`, though one that needs more memory
  than there is is not.
- `CacheFile::map`'s safety contract holds from the call on, the check's reads among
  those the file must not change under.
- `euler` returns the lowest-numbered leaf's error whatever the thread count, as
  `par_update` does; `par_update` states that a failure undoes nothing.

### Breaking

- A Hermite set whose sign grid shares a node, a leaf or a tile value between entries is
  refused, `FormatError::Shared` (docs/formats.md §4.2, rule 11): checking such a set
  walked the shared nodes once per entry, a small file asking for much work, and checked
  a shared leaf at one origin only. This refuses files version 1 accepted; brodo writes
  none (ADR-0038).
- A Hermite set whose fields hold more leaves or tiles than its signs is refused,
  `HermiteError::OtherTopology` (docs/formats.md §4.2, rule 3): the values beyond the
  signs' were reached by no node and checked by nothing. This refuses files version 1
  accepted; brodo writes none (ADR-0038).
- A voxel centre on the mesh, at distance exactly 0, is inside whichever sign is asked
  for, as in Hermite data, where the winding number turned on a rounded zero's sign; an
  inside voxel whose distance rounds to zero holds −`f32::MIN_POSITIVE`, not −0, so that
  `v < 0` finds it and parts sharing a face are one solid (ADR-0027).
- `Mesh::check_groups` refuses a group after 0 named `""` or by an earlier group's name,
  the new `MeshError::BadGroupName`, as Hermite data does: such a mesh converted and was
  refused only once its Hermite data was made.
- A Hermite set made with the group table `[""]` keeps it empty, as a file reads it back.
- `cache::write` writes a hidden file beside the path, `.<name>.<process id>.<n>.tmp`, n
  counting the process's writes, and renames it over the path once flushed: written in
  place, a file another process mapped changed under it, and a failed write lost the old
  file.
- `FreezeError::Inconsistent` is removed: nothing made it.
- `Converted` records the accuracy its winding numbers were found at, the new field
  `accuracy`, which `hermite` and the report's third layer follow: `hermite(mesh, c)` takes
  no accuracy of its own, and the third layer no longer uses β = 2 whatever the conversion
  used (ADR-0027).
- colino's OBJ reader takes a `g` naming no group as none: the triangles after it take the
  last `o`'s name, where they took group 0 (ADR-0030).
- `Grid::parts_mut` is removed: a grid's field is written through `Grid::par_update_leaves`,
  whose closure takes the topology, a leaf's number and its values, `Grid::leaf_mut` and
  `Grid::set_tile`, so that it cannot be replaced by another topology's (ADR-0014).
- A Hermite set has metadata of its own, `Produced::meta`: the conversion's lattice and
  unit, named `hermite`, without `sdf.version`. `Hermite::entries` refuses metadata
  stating the SDF convention, the new `HermiteError::ClaimsSdf`, and `CacheEntry::new`
  refuses a key beginning `hermite.`, the new `CacheError::ReservedKey`;
  `CacheEntry::with_reserved_keys` writes one on purpose (ADR-0030).
- A leaf or tile an edit puts where the same edit removed a leaf starts from the background
  (ADR-0014): a leaf made again had taken the removed leaf's values, and a lower or upper
  tile had merged them, refusing values not all equal.
- colino's ASCII STL reader takes a solid's name as the rest of its line (ADR-0030): a
  name may now hold `facet` or `endsolid`, which it refused, but a name written on the
  line after `solid` is refused, and the words after `endsolid` end with its line.
- `Value` no longer carries `TYPE_CODE`: a type of a user's own implements `Value` with
  `same_bits` alone, and serves in a field, its parallel update, gathers and analyses. The
  code moves to the new `brodo::field::Stored: Value`, sealed, implemented for brodo's six
  value types alone; `freeze`, `FrozenGrid::view`, `GridView`, `CacheEntry::new` and
  `CacheFile::view` take a `Stored` type, so that a cache file of a user's type, which no
  build read, and a user's type under brodo's code, read as brodo's, can no longer be
  written (ADR-0029). `VtkValue` is `Stored` with its VTK names.

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
- `Field::remap` takes the merge rule its merged tiles' values come from, such as
  `uniform`.
- Field values are bounded by `brodo::field::Value`, which requires `Copy + Send + Sync`
  and `zerocopy`'s `FromBytes`, `IntoBytes`, `Immutable` and `KnownLayout`.

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

## 0.1.0 (2026-10-16)

Closes P0: the repository with its rules, the `brodo` and `colino` crates (empty), and the
design. No API and no format yet.
