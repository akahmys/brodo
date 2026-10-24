# Provenance

What each source file under `crates/*/src/` drew on, and for what (AGENTS.md S3). Only
papers, public documentation and permissively licensed code (S1, S2) may appear here.

## References

| Key | Reference |
| :--- | :--- |
| Museth 2013 | K. Museth, "VDB: High-Resolution Sparse Volumes with Dynamic Topology", ACM Transactions on Graphics, 2013 |
| NanoVDB docs | NanoVDB's public documentation, for its data-structure ideas only |
| Jacobson 2013 | A. Jacobson et al., "Robust Inside-Outside Segmentation using Generalized Winding Numbers", ACM Transactions on Graphics, 2013 |
| Barill 2018 | G. Barill et al., "Fast Winding Numbers for Soups and Clouds", ACM Transactions on Graphics, 2018 |

## Files

| File | Draws on | For |
| :--- | :--- | :--- |
| `crates/brodo/src/accessor.rs` | ARCHITECTURE.md §4.4, ADR-0015 only | value accessors: reads and leaf-only writes from a cached path |
| `crates/brodo/src/analysis.rs` | ARCHITECTURE.md §4.5, ADR-0013 only; union-find and breadth-first search as textbook material | connected-component labelling and flood fill on the sparse tree |
| `crates/brodo/src/cache.rs` | ARCHITECTURE.md §4.7, ADR-0016 only | the cache profile: writing and checking a file of grids with their metadata |
| `crates/brodo/src/error.rs` | none | the crate's error types |
| `crates/brodo/src/euler.rs` | ARCHITECTURE.md §4.5, ADR-0024; the Euler–Poincaré formula over a voxel complex as stated by Sánchez-Cruz et al. (2013, J. Appl. Res. Technol. 11(1), 65–78); a sum over 2×2×2 neighbourhoods as ImageJ Ops' documentation of `EulerCharacteristic26N` describes it | the Euler characteristic of the voxels meeting a condition, under 6- or 26-connectivity |
| `crates/brodo/src/frozen.rs` | ARCHITECTURE.md §4.6, ADR-0016 only; `zerocopy`'s public API documentation (0.8.63) | the frozen layout, version 1, and `freeze` |
| `crates/brodo/src/gather.rs` | ARCHITECTURE.md §4.4, ADR-0015 only | the gather of a leaf and one, two or three ghost layers |
| `crates/brodo/src/field.rs` | ARCHITECTURE.md §4.2, ADR-0014 only | fields: the `Value` trait, values by leaf and tile number, the stamp check, the remap and merge rules |
| `crates/brodo/src/grid.rs` | ARCHITECTURE.md §4.2, ADR-0014 only | a grid: one topology and one field, edited in one step |
| `crates/brodo/src/hermite.rs` | ARCHITECTURE.md §4.9, ADR-0032; Ju, Losasso, Schaefer and Warren (2002), *Dual Contouring of Hermite Data*, for Hermite data as signs on a lattice and a crossing and normal on each edge whose ends differ | the Hermite data format: a set of ten fields, its check, its cache entries and its reading in place |
| `crates/brodo/src/lib.rs` | none | crate root |
| `crates/brodo/src/manifest.rs` | ARCHITECTURE.md §4.3, ADR-0014 only | the leaf manifest: 26 directions and each leaf's neighbours |
| `crates/brodo/src/mapping.rs` | ARCHITECTURE.md §4.7, ADR-0008 only; `memmap2`'s public API documentation (0.9.11) | mapping a file read-only: brodo's one unsafe operation |
| `crates/brodo/src/meta.rs` | ARCHITECTURE.md §4.7, ADR-0016 only | a grid's metadata and bounding box |
| `crates/brodo/src/morphology.rs` | ARCHITECTURE.md §4.5, ADR-0025 only; dilation and erosion by a structuring element as textbook material | dilation and erosion of a topology on whole leaves, and of values by a max or min filter |
| `crates/brodo/src/parallel.rs` | ARCHITECTURE.md §4.4, ADR-0015 only | the leaf-parallel update and the fixed-order reduction |
| `crates/brodo/src/topology.rs` | ARCHITECTURE.md §4.2, ADR-0014 only; Morton order as bit interleaving | topologies: stamps, Morton numbering, the editor, `Remap` |
| `crates/brodo/src/view.rs` | ARCHITECTURE.md §4.6, ADR-0016, ADR-0032 only; `zerocopy`'s public API documentation (0.8.63) | `GridView`: checking a frozen grid whole, reading it in place, and walking its leaves and tiles |
| `crates/brodo/src/slice.rs` | ARCHITECTURE.md §6.1, ADR-0018 only; `png`'s public API documentation (0.18.1) | a slice of a scalar field drawn and encoded as PNG |
| `crates/brodo/src/vtk.rs` | ARCHITECTURE.md §6.1, ADR-0018 only; VTK's published XML file-format documentation, its `NumberOfComponents` and `Vectors` included; VTK_VOXEL's number from VTK's public Python API | a field, scalar or of three components, written as one ASCII `.vtu` of voxel cells |
| `crates/brodo/src/tree.rs` | ARCHITECTURE.md §4.1, ADR-0013; Museth 2013, for a shallow, wide tree of fixed levels with tiles | the sparse tree: levels, masks, arenas, activation, tiles, their split and merge, iteration, compaction |
| `crates/colino/src/bvh.rs` | ARCHITECTURE.md §5.1, ADR-0027; a bounding volume hierarchy split at the median, as textbook material | a tree of axis-aligned boxes over a mesh's triangles, and the nearest triangle to a point |
| `crates/colino/src/convert.rs` | ARCHITECTURE.md §5.1, ADR-0027 only | a mesh converted into a narrow-band signed distance field, with inside tiles |
| `crates/colino/src/distance.rs` | ARCHITECTURE.md §5.1, ADR-0027; the nearest point of a triangle by its Voronoi regions, as textbook material | the nearest point of a segment and of a triangle |
| `crates/colino/src/intersect.rs` | ARCHITECTURE.md §5.1, ADR-0030; Woop, Benthin and Wald (2013), *Watertight Ray/Triangle Intersection*, JCGT 2(1), for the sheared 2D edge test and its tie rule; the nearest points of two segments by their stationary point and clamped sides, as textbook material | where a segment crosses a triangle, watertight, and the point of a segment nearest a triangle |
| `crates/colino/src/lib.rs` | none | crate root |
| `crates/colino/src/error.rs` | none | the crate's error types |
| `crates/colino/src/hermite.rs` | ARCHITECTURE.md §5.1, ADR-0030, ADR-0032 only | Hermite data from a mesh: signs by the winding number, crossings by the watertight test, in brodo's format |
| `crates/colino/src/mesh.rs` | ARCHITECTURE.md §5.1, ADR-0026 only | the mesh type, reading limits, and welding by exact position |
| `crates/colino/src/obj.rs` | ARCHITECTURE.md §5.1, ADR-0026; the Wavefront OBJ specification as published at paulbourke.net/dataformats/obj/ | reading Wavefront OBJ |
| `crates/colino/src/stl.rs` | ARCHITECTURE.md §5.1, ADR-0026; the Library of Congress's format descriptions fdd000505 (STL binary) and fdd000506 (STL ASCII) | reading binary and ASCII STL |
| `crates/colino/src/report.rs` | ARCHITECTURE.md §5.2, ADR-0028; Betti numbers, the Euler–Poincaré relation and Alexander duality in the 3-sphere, as textbook material | the topology report's first two layers: the solid's and the complement's Betti numbers, the input's reference and named points' components |
| `crates/colino/src/surface.rs` | ARCHITECTURE.md §5.2, ADR-0028; the Euler characteristic V − E + F of a closed 2-manifold and union-find, as textbook material | a mesh's surface counted, and whether it is a closed 2-manifold |
| `crates/colino/src/threemf.rs` | ARCHITECTURE.md §5.1, ADR-0026; the 3MF Core Specification 1.4.0 (3MF Consortium, 2025), its matrix figure included; `rawzip` 0.5.2's, `miniz_oxide` 0.9.1's and `quick-xml` 0.42.0's public API documentation | reading 3MF packages |
| `crates/colino/src/winding.rs` | ARCHITECTURE.md §5.1, ADR-0027; Jacobson, Kavan and Sorkine-Hornung (2013), equations 5 and 6 (van Oosterom and Strackee's solid angle); Barill, Dickson, Schmidt, Levin and Jacobson (2018), Algorithm 1, equations 18 to 29 and appendices A and B | the generalised winding number, exact and by the far-field approximation |
