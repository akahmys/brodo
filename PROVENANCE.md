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
| `crates/brodo/src/error.rs` | none | the crate's error types |
| `crates/brodo/src/field.rs` | ARCHITECTURE.md §4.2, ADR-0014 only | fields: values by leaf and tile number, the stamp check, the remap |
| `crates/brodo/src/lib.rs` | none | crate root |
| `crates/brodo/src/topology.rs` | ARCHITECTURE.md §4.2, ADR-0014 only; Morton order as bit interleaving | topologies: stamps, Morton numbering, the editor, `Remap` |
| `crates/brodo/src/tree.rs` | ARCHITECTURE.md §4.1, ADR-0013 only | the sparse tree: levels, masks, arenas, activation, tiles and their split, iteration |
| `crates/colino/src/lib.rs` | none | crate root |
