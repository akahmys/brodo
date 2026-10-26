#![doc = include_str!("../README.md")]
//!
//! The design is in the repository's `ARCHITECTURE.md` §5.

pub mod bvh;
pub mod convert;
pub mod distance;
pub mod error;
pub mod hermite;
pub mod intersect;
pub mod mesh;
#[cfg(feature = "obj")]
pub mod obj;
mod parity;
pub mod report;
#[cfg(feature = "stl")]
pub mod stl;
pub mod surface;
#[cfg(feature = "threemf")]
pub mod threemf;
pub mod winding;
