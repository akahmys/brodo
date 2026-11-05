# ADR-0047: How brodo reads `.vdb` files

- **Status**: Amended
- **Date**: 2026-11-01

## Context

ADR-0045 had brodo read the files it writes and those OpenVDB 12 to 14 writes for float
and double grids by default, after writing (ADR-0046). The spike `spikes/vdbread/`
(2026-11-01, `spikes/vdbread/REPORT.md`) built OpenVDB 12.0.0 and 13.0.0 from their
Apache-2.0 tags beside Homebrew's 14.0.0 (PROVENANCE.md) and had each write five float and
double grids by default, with zip and with none:

- All three write Blosc with active-mask by default; 12.0.0 at file version 224, 13.0.0 and
  14.0.0 at 225, laid out the same for these grids.
- Each library read every file, brodo's golden file included, with the same values. 12.0.0
  warned on version 225 and read it the same; stamped 224, brodo's bytes read without a
  warning in all three.
- A reader written from docs/formats.md §9, the public LZ4 and Blosc chunk formats and
  OpenVDB's output read all ten files line for line as OpenVDB 14.0.0 lists them.
- OpenVDB's files hold the seven per-node indicators of inactive values; Blosc chunks of
  LZ4 with byte shuffle, or stored, always one block; maps other than `AffineMap`, its
  default `UniformScaleMap`; metadata of types brodo does not know; and inactive voxels and
  tiles, some holding a value other than the background. Zip was never written by default,
  nor on a level set or fog volume.

## Decision

The owner decided, on 2026-11-01, as proposed:

- **Versions**: file versions 224 and 225 are read; any other is refused. brodo goes on
  writing 225 (ADR-0046).
- **Compression**: none, active-mask, and Blosc of LZ4 with byte shuffle or stored, in
  one block, decoded with `lz4_flex` (ADR-0016), so that reading adds no dependency. Zip,
  and any other Blosc chunk, is refused until a user asks (ADR-0036).
- **Activity**: brodo holds none (ADR-0025). A leaf's inactive voxels keep their values;
  an active tile, and an inactive tile holding other than the background, become brodo's
  tile of that value; an inactive tile holding the background is left out.
- **Transform**: `AffineMap`, `ScaleMap`, `UniformScaleMap` and `UniformScaleTranslateMap`,
  the maps whose bytes were measured, are read into brodo's 3×4 map with the half voxel
  ADR-0046 adds taken away; any other map is refused until measured.
- **Metadata**: a grid's name and transform are taken, its voxel size as the length of
  each of the map's columns. OpenVDB states no unit, so the caller gives one when reading.
  String entries are kept in `extra`; OpenVDB's own `file_*` entries, and entries of other
  types, are dropped. `sdf.version` is not set, since OpenVDB's level sets are not known to
  follow brodo's convention (docs/formats.md §5).
- **Other grids**: a grid of another type, saved as half floats, or instanced, is skipped,
  and its name returned, so that none is lost unnoticed.
- **Untrusted input**: the reader joins `UNTRUSTED_INPUT` and the closing fuzz round.

The bytes read are specified in docs/formats.md §9.

## Consequences

brodo reads the files OpenVDB 12 to 14 writes by default without a new dependency, and
loses on reading what it cannot hold: which voxels were active. A file written again by
brodo holds every voxel active (ADR-0046). Files with zip, other Blosc codecs, half floats
or other maps are refused or skipped until a user asks and a spike measures them. OpenVDB's
tools stay the check, by hand, not in the gate.

## Amendment: the reader built

- **Date**: 2026-11-01

The reader was built as decided, in `crates/brodo/src/vdb_read.rs`, a module of its own so
that the writer is not held to CODING.md Rule 3; it was listed in `UNTRUSTED_INPUT`
(`scripts/audit/project.py`) and given the fuzz target `fuzz/fuzz_targets/vdb.rs`, as the
decision had it. Two facts the spike had not met were taken from OpenVDB's output while
building its test files: OpenVDB writes a grid saved as half floats with `_HalfFloat`
appended to its type name, so that it is skipped as a type brodo does not read, and an
instanced grid with its leaf buffers' offset 0. docs/formats.md §9.7 and §9.8 state the
bytes; `crates/brodo/tests/vdb_read.rs` holds the reader to OpenVDB's own reading of files
OpenVDB 12.0.0 and 14.0.0 wrote.

## Amendment: the memory reading makes

- **Date**: 2026-11-02

An outside review (2026-11-01) found that `vdb::read`'s `limit` bounded the file's length
alone, where `archive::read`'s bounds what reading makes too (ADR-0036). The spike
`spikes/vdbmemory/` (2026-11-02, `spikes/vdbmemory/REPORT.md`) measured the file that makes
the most for each byte: upper nodes of inactive tiles alone, a quarter of a byte a tile,
made about 637 bytes for each byte of an `f64` file and 617 of an `f32` one; leaves holding
the background, 68.

The owner decided, on 2026-11-02, as proposed: the multiple is stated now, in
ARCHITECTURE.md §4.7, and `limit` bounding what `vdb::from_bytes` and `vdb::read` make, as
`archive` does, is a unit of its own after the review of the whole repository, since it
changes public items (ROADMAP.md §2, "After P7").

## Amendment: the limit counts what reading makes

- **Date**: 2026-11-03

What reading makes could not be bounded as `archive` bounds it, by the bytes of the cache
file of the same grids: a cache file holds 32 768 tiles in one upper node of 8224 bytes,
where reading made about 160 bytes a tile (measured 2026-11-03, the cases of
`spikes/vdbmemory/` with an `f32` leaf case and node cases added). The owner chose, on
2026-11-03, to count each thing as it is read, at what it makes at most, against the limit,
refusing before it is made. `vdb::from_bytes` takes the limit, as `archive::from_bytes`
does, and the fuzz target `vdb` passes one.

The counts were set from the measurement: a leaf twice its values' bytes, the reader's map
and the field holding them, and 1024 more; a tile 256, against about 160 made; a node its
two masks twice, the editor's tree and the one it commits, and room for the rest, 2560 a
lower node and 18 432 an upper node. Each case made less than it was counted, beside the
file's bytes, the least margin on upper nodes, 617 MB made against 696 MB counted.

## Amendment: a level set read as an SDF

- **Date**: 2026-11-06

T14's survey (2026-11-05) found that an SDF brodo writes to a `.vdb`, marked `class` =
`level set` (docs/formats.md §9.3), read back without `brodo.sdf.version`, which the
decision above withheld from every grid, and that a level set OpenVDB wrote, read and
written again by brodo, became `class` = `unknown`, since the writer derives `class` from
that key alone. The spike `spikes/vdbclass/` (2026-11-06, `spikes/vdbclass/REPORT.md`) had
OpenVDB 14.0.0's tools make fifteen level sets, of spheres, a cube, an icosahedron, CSG,
two filters, a rebuild and meshes of up to 101 760 triangles under uniform, translated,
rotated and non-uniform maps. Every one held its leaf values within ±background, not one
by a step outside, and its tiles at ±background; the thirteen of `f32` with uniform maps
met every condition below. Under 2000 rotated maps OpenVDB wrote, and 100 000 brodo wrote,
the three voxel sizes brodo reads were exactly equal for 45% and 25% of grids, and at most
2.25 ε and 4.38 ε apart, relative to the least.

The owner decided, on 2026-11-06, as proposed: a grid of `f32` whose `class` is
`level set` is read as stating the convention, `class` replaced by `brodo.sdf.version` =
`2`, when its voxel sizes lie within 16 ε of each other, relative to the least, 3.6 times
the most measured; its background is positive and finite; every leaf value lies within
±background, none NaN; and every tile kept is ±background. Any other grid keeps `class`
as before, a level set of `f64` among them, since §5 holds `f32` alone. That the values
are signed distances, negative inside, cannot be checked, and is taken from OpenVDB's
convention for level sets. ARCHITECTURE.md §4.7 states the conditions.
