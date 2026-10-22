#![doc = include_str!("../README.md")]
//!
//! The design is in the repository's `ARCHITECTURE.md` §4.

pub mod accessor;
pub mod analysis;
pub mod cache;
pub mod error;
pub mod euler;
pub mod field;
pub mod frozen;
pub mod gather;
pub mod grid;
pub mod hermite;
pub mod manifest;
mod mapping;
pub mod meta;
pub mod morphology;
pub mod parallel;
#[cfg(feature = "slice-png")]
pub mod slice;
pub mod topology;
pub mod tree;
pub mod view;
#[cfg(feature = "vtk")]
pub mod vtk;
