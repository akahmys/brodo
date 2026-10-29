#![doc = include_str!("../README.md")]
//!
//! The design is in the repository's `ARCHITECTURE.md` §4.

// A leaf's values start at its number times 512, which a `usize` of 64 bits holds for every
// leaf number a `u32` gives; brodo builds for 64-bit targets alone.
#[cfg(not(target_pointer_width = "64"))]
compile_error!("brodo builds for 64-bit targets alone");

pub mod accessor;
pub mod analysis;
#[cfg(feature = "archive")]
pub mod archive;
pub mod cache;
pub mod error;
pub mod euler;
pub mod field;
pub mod frozen;
pub mod gather;
pub mod grid;
pub mod hermite;
pub mod manifest;
#[cfg(feature = "mmap")]
mod mapping;
pub mod meta;
pub mod morphology;
pub mod parallel;
#[cfg(feature = "archive")]
mod quantise;
#[cfg(feature = "slice-png")]
pub mod slice;
pub mod topology;
pub mod tree;
pub mod view;
#[cfg(feature = "vtk")]
pub mod vtk;
