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
    /// A leaf number the tree or topology did not give.
    #[error("no leaf numbered {0}")]
    UnknownLeaf(u32),
    /// A tile number the topology did not give.
    #[error("no tile numbered {0}")]
    UnknownTile(u32),
    /// More tiles than a `u32` numbers.
    #[error("more tiles than a u32 numbers")]
    TooManyTiles,
}

/// Why a field refused an access or a remap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FieldError {
    /// The field belongs to another topology: it was not remapped after a commit, or the
    /// remap is not from its topology (ADR-0014).
    #[error("the field belongs to another topology")]
    StaleField,
    /// A leaf number its topology did not give.
    #[error("no leaf numbered {0}")]
    UnknownLeaf(u32),
    /// A tile number its topology did not give.
    #[error("no tile numbered {0}")]
    UnknownTile(u32),
}
