//! The errors of brodo's operations (CODING.md Rule 8).

use crate::tree::{Coord, TileLevel};

/// The kinds of node a tree keeps in arenas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// Internal nodes of 32³ children.
    Upper,
    /// Internal nodes of 16³ children.
    Lower,
    /// Leaves of 8³ voxels.
    Leaf,
}

/// Why an edit of the tree was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TreeError {
    /// No tile contains the voxel to split at.
    #[error("no tile contains voxel {0:?}")]
    NotATile(Coord),
    /// The region a tile was to fill already holds a tile, a node or a leaf, or lies in a
    /// tile.
    #[error("the {level:?} region containing voxel {voxel:?} is not empty")]
    Occupied {
        /// The level of the tile asked for.
        level: TileLevel,
        /// The voxel given.
        voxel: Coord,
    },
    /// An arena would hold more nodes than a `u32` numbers.
    #[error("more {0:?} nodes than a u32 numbers")]
    ArenaFull(NodeKind),
    /// A leaf number this tree did not give.
    #[error("no leaf numbered {0}")]
    UnknownLeaf(u32),
}
