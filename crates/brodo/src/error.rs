//! The errors of brodo's operations (CODING.md Rule 8).

use crate::topology::Location;
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
    /// More merged parts in one commit than a `u32` numbers.
    #[error("more merged parts than a u32 numbers")]
    TooManyParts,
    /// No child holds the voxel where a merge into a tile at this level was asked for.
    #[error("nothing to merge into a {level:?} tile at voxel {voxel:?}")]
    NotMergeable {
        /// The level of the tile asked for.
        level: TileLevel,
        /// The voxel given.
        voxel: Coord,
    },
    /// The node to merge holds a place with nothing in it (ADR-0014).
    #[error("the node merging into a {level:?} tile at voxel {voxel:?} holds an empty place")]
    HoldsEmpty {
        /// The level of the tile asked for.
        level: TileLevel,
        /// The voxel given.
        voxel: Coord,
    },
    /// Merging into a tile at this level is not built (ADR-0014).
    #[error("merging into a {0:?} tile is not built")]
    MergeUnsupported(TileLevel),
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
    /// The merge rule refused the values of the new tile with this number.
    #[error("the merge rule refused new tile {0}")]
    MergeRefused(u32),
    /// A write to a voxel no leaf holds: values change only in leaves (ADR-0015).
    #[error("voxel {voxel:?} is not in a leaf but in {location:?}")]
    NotALeaf {
        /// The voxel written.
        voxel: Coord,
        /// What holds it.
        location: Location,
    },
}

/// Why a grid could not be frozen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FreezeError {
    /// The field does not belong to the topology.
    #[error(transparent)]
    Field(#[from] FieldError),
    /// The topology refused a lookup by number.
    #[error(transparent)]
    Tree(#[from] TreeError),
    /// A count or offset does not fit the layout's `u32` words.
    #[error("the grid is too large for the frozen layout")]
    TooLarge,
    /// The topology's lists and its tree disagree.
    #[error("the topology's lists and its tree disagree")]
    Inconsistent,
}

/// Why bytes were refused as a frozen grid (ARCHITECTURE.md §4.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FormatError {
    /// The bytes do not start on a 32-byte boundary, or are not whole blocks.
    #[error("the bytes are not 32-byte aligned whole blocks")]
    Misaligned,
    /// Too few bytes for the header or a section.
    #[error("the bytes end before the {0} section does")]
    Truncated(&'static str),
    /// Not a frozen grid.
    #[error("magic number {0:#010x} is not a frozen grid's")]
    BadMagic(u32),
    /// A layout version this build does not read (ADR-0016).
    #[error("layout version {0} is not read by this build")]
    UnsupportedVersion(u32),
    /// The values are of another type.
    #[error("the values are of type {found}, not {expected}")]
    WrongValueType {
        /// The type code asked for.
        expected: u32,
        /// The type code found.
        found: u32,
    },
    /// A count, offset or index points outside its section, or contradicts another.
    #[error("the {0} section is inconsistent")]
    Inconsistent(&'static str),
}
