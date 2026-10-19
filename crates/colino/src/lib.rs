//! Conversion of external shapes (triangle meshes) to narrow-band signed distance fields on
//! brodo's sparse tree, with a report of the topology changes the conversion may cause.
//! The design is in `ARCHITECTURE.md` §5. Meshes are read so far (ROADMAP.md, P2).

pub mod error;
pub mod mesh;
pub mod obj;
pub mod stl;
pub mod threemf;
