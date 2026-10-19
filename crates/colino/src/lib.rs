//! Conversion of external shapes (triangle meshes) to narrow-band signed distance fields on
//! brodo's sparse tree, with a report of the topology changes the conversion may cause.
//! The design is in `ARCHITECTURE.md` §5. Meshes are read, and distances to them found, so
//! far (ROADMAP.md, P2).

pub mod bvh;
pub mod convert;
pub mod distance;
pub mod error;
pub mod mesh;
pub mod obj;
pub mod stl;
pub mod surface;
pub mod threemf;
pub mod winding;
