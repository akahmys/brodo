# Provenance

What each source file under `crates/*/src/` drew on, and for what (AGENTS.md S3). Only
papers, public documentation and permissively licensed code (S1, S2) may appear here.
Every file also draws on the public API documentation of the dependencies its crate's
`Cargo.toml` names, at the versions `Cargo.lock` pins; a row names what else it draws on.

## References

| Key | Reference |
| :--- | :--- |
| Museth 2013 | K. Museth, "VDB: High-Resolution Sparse Volumes with Dynamic Topology", ACM Transactions on Graphics, 2013 |
| Jacobson 2013 | A. Jacobson et al., "Robust Inside-Outside Segmentation using Generalized Winding Numbers", ACM Transactions on Graphics, 2013 |
| Barill 2018 | G. Barill et al., "Fast Winding Numbers for Soups and Clouds", ACM Transactions on Graphics, 2018 |

## OpenVDB, read for the `.vdb` format

What was read of OpenVDB, at a tag from `v12.0.0` on, and for what (AGENTS.md S1,
ADR-0044). Its code is neither copied nor translated; the format is written in
docs/formats.md.

| Tag | Commit | File | For |
| :--- | :--- | :--- | :--- |
| `v12.0.0` | `269300808a4651daa65836214fc20f78e1c0eb7f` | `LICENSE`, `README.md`, `RE-LICENSE_NOTE.txt` | The licence (ADR-0044) |
| `v12.0.0` | `269300808a4651daa65836214fc20f78e1c0eb7f` | The archive built, not read; the installed `openvdb/version.h`'s line stating file version 224, shown by the compiler | OpenVDB 12.0.0 as a reader and writer in `spikes/vdbread/` |
| `v13.0.0` | `7c03e1f084873cd1b3422c7ff7aec6ee681b3b38` | The archive built, not read; `LICENSE` the same bytes as at `v14.0.0` by SHA-256, and `RE-LICENSE_NOTE.txt` the same as at `v12.0.0`; `openvdb/openvdb/io/Archive.cc`, `Archive::setGridCompression` | OpenVDB 13.0.0 as a reader and writer in `spikes/vdbread/`; why zip is dropped on level sets and fog volumes |
| `v14.0.0` | `ecae2bda85744412b0cd47e1b8e1e4e76beb7b88` | `LICENSE`, `README.md`, `RE-LICENSE_NOTE.txt` | The licence (ADR-0044) |
| `v14.0.0` | `ecae2bda85744412b0cd47e1b8e1e4e76beb7b88` | Under `openvdb/openvdb/`: `COPYRIGHT`; `io/` `File.cc`, `Archive.h`, `Archive.cc`, `GridDescriptor.cc`, `Compression.h`, `Compression.cc`, `Codec.h`, `Codec.cc`, `io.h`; `codecs/` `ScalarCodec.h`, `TopologyCodec.h`, `impl/ScalarLeafCodec.h`; `Grid.cc`, `MetaMap.cc`, `Metadata.h`, `Metadata.cc`, `version.h.in`; `math/` `Maps.h`, `Maps.cc`, `Transform.cc` in part; `util/NodeMasks.h` in part; `tree/` `Tree.h`, `RootNode.h`, `InternalNode.h`, `LeafNode.h` in part | The layout a writer emits for float and double grids at file version 225 (`spikes/vdb/`, ADR-0045) |

## Code copied from other crates

None (AGENTS.md S2): no file under `crates/*/src/` carries another project's licence or
copyright notice (`grep -rniE 'copyright|spdx|licen[cs]e' crates/*/src` finds none,
2026-11-02).

## Files

| File | Draws on | For |
| :--- | :--- | :--- |
| `crates/brodo/src/accessor.rs` | ARCHITECTURE.md §4.4, ADR-0015 only | value accessors: reads and leaf-only writes from a cached path |
| `crates/brodo/src/analysis.rs` | ARCHITECTURE.md §4.5, ADR-0013 only; union-find and breadth-first search as textbook material | connected-component labelling and flood fill on the sparse tree |
| `crates/brodo/src/analysis/euler.rs` | ARCHITECTURE.md §4.5, ADR-0024; the Euler–Poincaré formula over a voxel complex as stated by Sánchez-Cruz et al. (2013, J. Appl. Res. Technol. 11(1), 65–78); a sum over 2×2×2 neighbourhoods as ImageJ Ops' documentation of `EulerCharacteristic26N` describes it | the Euler characteristic of the voxels meeting a condition, under 6- or 26-connectivity |
| `crates/brodo/src/archive.rs` | ARCHITECTURE.md §4.7, ADR-0016, ADR-0051, ADR-0052; the LZ4 project's `lz4_Block_format.md`, for the block format docs/formats.md §8.3 names | the archive profile: writing grids in compressed chunks and reading them back as a cache file, or one grid's region of them |
| `crates/brodo/src/quantise.rs` | docs/formats.md §8.5, ADR-0016; IEEE 754-2019 §3.6, for binary16's format and §4.3.3, for rounding to nearest, ties to even | an archive's quantised leaf values: binary16, byte-shuffled, and 8 bits over a range |
| `crates/brodo/src/cache.rs` | ARCHITECTURE.md §4.7, ADR-0016, ADR-0051, ADR-0052, ADR-0057 only | the cache profile: writing and checking a file of grids with their metadata, and reaching each grid read |
| `crates/brodo/src/error.rs` | ADR-0058 only | the crate's error types |
| `crates/brodo/src/frozen.rs` | ARCHITECTURE.md §4.6, ADR-0016, ADR-0052 only | the frozen layout, version 1, and `freeze` |
| `crates/brodo/src/gather.rs` | ARCHITECTURE.md §4.4, ADR-0015 only | the gather of a leaf and one, two or three ghost layers |
| `crates/brodo/src/field.rs` | ARCHITECTURE.md §4.2, §4.8, ADR-0014, ADR-0056 only | fields: the open `Value` trait and the sealed `Stored` one, the `ValueType` naming each stored type and the `AnyField` holding a field of one, values by leaf and tile number, the stamp check, the remap and merge rules |
| `crates/brodo/src/grid.rs` | ARCHITECTURE.md §4.2, ADR-0014 only | a grid: one topology and one field, edited in one step |
| `crates/brodo/src/kind.rs` | ARCHITECTURE.md §4.7, ADR-0056; docs/formats.md §2.1, §3.1, §8.1 and §9.1 for each file's first bytes | a file's kind told by its magic number, and each format's magic number |
| `crates/brodo/src/hermite.rs` | ARCHITECTURE.md §4.9, ADR-0032, ADR-0051; Ju, Losasso, Schaefer and Warren (2002), *Dual Contouring of Hermite Data*, for Hermite data as signs on a lattice and a crossing and normal on each edge whose ends differ | the Hermite data format: a sign grid and its crossings held sparsely, its check, its cache entries and its reading in place |
| `crates/brodo/src/lib.rs` | none | crate root |
| `crates/brodo/src/manifest.rs` | ARCHITECTURE.md §4.3, ADR-0014 only | the leaf manifest: 26 directions and each leaf's neighbours |
| `crates/brodo/src/mapping.rs` | ARCHITECTURE.md §4.7, ADR-0008 only | mapping a file read-only: brodo's one unsafe operation |
| `crates/brodo/src/meta.rs` | ARCHITECTURE.md §4.7, ADR-0016 only | a grid's metadata and bounding box |
| `crates/brodo/src/morphology.rs` | ARCHITECTURE.md §4.5, ADR-0025 only; dilation and erosion by a structuring element as textbook material | dilation and erosion of a topology on whole leaves, and of values by a max or min filter |
| `crates/brodo/src/parallel.rs` | ARCHITECTURE.md §4.4, ADR-0015 only | the leaf-parallel update and the fixed-order reduction |
| `crates/brodo/src/topology.rs` | ARCHITECTURE.md §4.2, ADR-0014 only; Morton order as bit interleaving | topologies: stamps, Morton numbering, the editor, `Remap` |
| `crates/brodo/src/view.rs` | ARCHITECTURE.md §4.6, ADR-0016, ADR-0032, ADR-0052 only | `GridView`: checking a frozen grid whole, or its nodes without its leaf values, reading it in place, and walking its leaves and tiles |
| `crates/brodo/src/slice.rs` | ARCHITECTURE.md §6.1, ADR-0018 only | a slice of a scalar field drawn and encoded as PNG |
| `crates/brodo/src/vdb.rs` | ARCHITECTURE.md §4.7, ADR-0045, ADR-0046, ADR-0047; docs/formats.md §9, from OpenVDB `v14.0.0`'s files listed above, read and not copied; `spikes/vdb/` | grids of `f32` and `f64` written as OpenVDB's `.vdb` file at file version 225, and `vdb_read.rs`'s reader re-exported |
| `crates/brodo/src/vdb_read.rs` | ARCHITECTURE.md §4.7, ADR-0047; docs/formats.md §9.7 and §9.8, from OpenVDB `v14.0.0`'s `io/Compression.h`, read and not copied, and OpenVDB's output in `spikes/vdbread/`; c-blosc's public `README_CHUNK_FORMAT.rst`, for a Blosc chunk; the LZ4 project's `lz4_Block_format.md`, its blocks decoded by `lz4_flex` | the float and double grids of OpenVDB files at file version 224 or 225 read, uncompressed, with active-mask or with Blosc's LZ4 |
| `crates/brodo/src/vtk.rs` | ARCHITECTURE.md §6.1, ADR-0018 only; VTK's published XML file-format documentation, its `NumberOfComponents` and `Vectors` included; VTK_VOXEL's number from VTK's public Python API | a field, scalar or of three components, written as one ASCII `.vtu` of voxel cells |
| `crates/brodo/src/tree.rs` | ARCHITECTURE.md §4.1, ADR-0013; Museth 2013, for a shallow, wide tree of fixed levels with tiles | the sparse tree: levels, masks, arenas, activation, tiles, their split and merge, iteration, compaction |
| `crates/colino/src/bvh.rs` | ARCHITECTURE.md §5.1, ADR-0027; a bounding volume hierarchy split at the median, as textbook material | a tree of axis-aligned boxes over a mesh's triangles, the nearest triangle to a point and to a segment, and a segment's first crossing, by the slab test, as textbook material |
| `crates/colino/src/convert.rs` | ARCHITECTURE.md §5.1, ADR-0027 only | a mesh converted into a narrow-band signed distance field, with inside tiles |
| `crates/colino/src/distance.rs` | ARCHITECTURE.md §5.1, ADR-0027; the nearest point of a triangle by its Voronoi regions, as textbook material | the nearest point of a segment and of a triangle |
| `crates/colino/src/intersect.rs` | ARCHITECTURE.md §5.1, ADR-0030; Woop, Benthin and Wald (2013), *Watertight Ray/Triangle Intersection*, JCGT 2(1), for the sheared 2D edge test and its tie rule; the nearest points of two segments by their stationary point and clamped sides, as textbook material | where a segment crosses a triangle, watertight, and the point of a segment nearest a triangle |
| `crates/colino/src/lib.rs` | none | crate root |
| `crates/colino/src/error.rs` | none | the crate's error types |
| `crates/colino/src/hermite.rs` | ARCHITECTURE.md §5.1, ADR-0030, ADR-0032 only | Hermite data from a mesh: signs by the winding number, crossings by the watertight test, in brodo's format |
| `crates/colino/src/mesh.rs` | ARCHITECTURE.md §5.1, ADR-0026 only | the mesh type, reading limits, and welding by exact position |
| `crates/colino/src/obj.rs` | ARCHITECTURE.md §5.1, ADR-0026; the Wavefront OBJ specification as published at paulbourke.net/dataformats/obj/ | reading Wavefront OBJ |
| `crates/colino/src/parity.rs` | ARCHITECTURE.md §5.1, ADR-0027; Nooruddin and Turk (2003), *Simplification and Repair of Polygonal Models Using Volumetric Techniques*, IEEE TVCG 9(2), §3.1, for the parity count's vote along several directions, a line with an odd count not voting; Woop, Benthin and Wald (2013), JCGT 2(1), for the 2D edge functions after moving the triangle to the line; the sign of an edge function exactly 0 taken with the line moved by (ε, ε²), derived in `spikes/sign/` | the sign that does not depend on orientation: a parity count along the three axes |
| `crates/colino/src/stl.rs` | ARCHITECTURE.md §5.1, ADR-0026; the Library of Congress's format descriptions fdd000505 (STL binary) and fdd000506 (STL ASCII) | reading binary and ASCII STL |
| `crates/colino/src/report.rs` | ARCHITECTURE.md §5.2, ADR-0028; Betti numbers, the Euler–Poincaré relation and Alexander duality in the 3-sphere, as textbook material | the topology report's first two layers: the solid's and the complement's Betti numbers, the input's reference and named points' components |
| `crates/colino/src/surface.rs` | ARCHITECTURE.md §5.2, ADR-0028; the Euler characteristic V − E + F of a closed 2-manifold and union-find, as textbook material | a mesh's surface counted, and whether it is a closed 2-manifold |
| `crates/colino/src/thin.rs` | ARCHITECTURE.md §5.2, ADR-0028; Cabiddu and Attene (2017), *Epsilon-shapes*, Computers & Graphics 66, §2.2, for the shape diameter function of Shapira et al. as a ray from a surface point along its inverted normal; union-find, as textbook material; brodo's Euler characteristic and labelling for the check | the topology report's third layer: walls thinner, and gaps narrower, than two voxels, each checked by its box's invariants at h and at h/2 |
| `crates/colino/src/threemf.rs` | ARCHITECTURE.md §5.1, ADR-0026; the 3MF Core Specification 1.4.0 (3MF Consortium, 2025), its matrix figure included | reading 3MF packages |
| `crates/colino/src/winding.rs` | ARCHITECTURE.md §5.1, ADR-0027; Jacobson 2013, equations 5 and 6 (van Oosterom and Strackee's solid angle); Barill 2018, Algorithm 1, equations 18 to 29 and appendices A and B | the generalised winding number, exact and by the far-field approximation |
| `crates/mestolo/src/lib.rs` | none | crate root: the usage, and each command run |
| `crates/mestolo/src/main.rs` | none | the program: its arguments run, its error and exit status |
| `crates/mestolo/src/args.rs` | ARCHITECTURE.md §6.3, ADR-0050 only | the command line parsed with `std` |
| `crates/mestolo/src/error.rs` | none | the crate's error type |
| `crates/mestolo/src/load.rs` | ARCHITECTURE.md §6.3, ADR-0050, ADR-0056 only | a file's kind told and its grids read and thawed through brodo, within the limit |
| `crates/mestolo/src/info.rs` | ARCHITECTURE.md §6.3, ADR-0050 only | what a file holds, as text |
| `crates/mestolo/src/draw.rs` | ARCHITECTURE.md §6.1, §6.3, ADR-0050; docs/formats.md §9 for OpenVDB's `class` of `level set` | a plane of a scalar grid drawn as PNG |
| `crates/mestolo/src/convert.rs` | ARCHITECTURE.md §6.3, ADR-0050 only | grids written as `.vdb` or one as `.vtu` |
| `crates/mestolo/src/output.rs` | ARCHITECTURE.md §6.3, ADR-0050 only | an output refused if a file is there or its path names none |
