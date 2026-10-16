//! Spike for B-001 and B-008 (ROADMAP.md §3): three ways to refuse a field not remapped after
//! a topology commit, over one shared remap that says where each new leaf's and tile's
//! values come from. The report is `REPORT.md`.

pub mod brand;
pub mod core;
pub mod fieldset;
pub mod generation;
