//! Sparse volumes: a VDB-style sparse tree, fields that share one topology, and a frozen,
//! zero-copy view of a grid. The design is in `ARCHITECTURE.md` §4. The tree, topologies
//! and fields are in place; the rest follows (ROADMAP.md, P1).

pub mod accessor;
pub mod analysis;
pub mod cache;
pub mod error;
pub mod field;
pub mod frozen;
pub mod gather;
pub mod grid;
pub mod manifest;
mod mapping;
pub mod meta;
pub mod parallel;
#[cfg(feature = "slice-png")]
pub mod slice;
pub mod topology;
pub mod tree;
pub mod view;
#[cfg(feature = "vtk")]
pub mod vtk;
