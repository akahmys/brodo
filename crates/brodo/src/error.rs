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
    /// A tile holds the voxel to deactivate; it must be split first (ADR-0025).
    #[error("a {level:?} tile holds voxel {voxel:?}; split it before deactivating")]
    InTile {
        /// The tile's level.
        level: TileLevel,
        /// The voxel given.
        voxel: Coord,
    },
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
    /// A count or offset does not fit the layout's `u32` words, or a value its header's
    /// background.
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

/// Why a grid's metadata was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MetaError {
    /// The unit is empty: a file always stores its physical size (ADR-0016).
    #[error("the unit is empty")]
    EmptyUnit,
    /// A voxel size is not finite and positive.
    #[error("a voxel size is not finite and positive")]
    BadVoxelSize,
    /// A transform entry is not finite.
    #[error("a transform entry is not finite")]
    BadTransform,
}

/// Why a cache file could not be written or read (ARCHITECTURE.md §4.7).
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    /// Reading or writing the file failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// A grid could not be frozen.
    #[error(transparent)]
    Freeze(#[from] FreezeError),
    /// A grid's metadata was refused.
    #[error(transparent)]
    Meta(#[from] MetaError),
    /// The topology refused a lookup by number.
    #[error(transparent)]
    Tree(#[from] TreeError),
    /// Not a cache file.
    #[error("magic number {0:#010x} is not a cache file's")]
    BadMagic(u32),
    /// A format version this build does not read (ADR-0016).
    #[error("cache format version {0} is not read by this build")]
    UnsupportedVersion(u32),
    /// The file ends before a part of it does.
    #[error("the file ends before its {0} does")]
    Truncated(&'static str),
    /// A part of the file contradicts another, or overlaps one.
    #[error("the file's {0} is inconsistent")]
    Inconsistent(&'static str),
    /// A string is not UTF-8.
    #[error("a string in the {0} is not UTF-8")]
    BadText(&'static str),
    /// A grid's frozen buffer was refused.
    #[error("grid {index}: {error}")]
    Grid {
        /// The grid's place in the file.
        index: usize,
        /// Why its buffer was refused.
        error: FormatError,
    },
    /// No grid has this place in the file.
    #[error("no grid numbered {0}")]
    NoGrid(usize),
    /// A length does not fit the format's `u32` fields.
    #[error("the file is too large for its u32 fields")]
    TooLarge,
}

/// Why a slice could not be drawn or encoded (ARCHITECTURE.md §6.1).
#[derive(Debug, thiserror::Error)]
pub enum SliceError {
    /// The field does not belong to the topology.
    #[error(transparent)]
    Field(#[from] FieldError),
    /// The range drawn holds no pixel.
    #[error("the slice's range holds no pixel")]
    EmptyRange,
    /// The colour map's range is not finite, or is empty (ARCHITECTURE.md §6.1).
    #[cfg(feature = "slice-png")]
    #[error("the colour map {0:?} has no finite, non-empty range")]
    BadColorMap(crate::slice::ColorMap),
    /// The PNG encoder refused the image.
    #[cfg(feature = "slice-png")]
    #[error(transparent)]
    Encode(#[from] png::EncodingError),
}

/// Why a field could not be written as `.vtu` (ARCHITECTURE.md §6.1).
#[derive(Debug, thiserror::Error)]
pub enum VtkError {
    /// The field does not belong to the topology.
    #[error(transparent)]
    Field(#[from] FieldError),
    /// The topology refused a lookup by number.
    #[error(transparent)]
    Tree(#[from] TreeError),
    /// The name is empty, or holds a character other than printable ASCII and space, or one
    /// of `<>&"'`.
    #[error("the name {0:?} is not one or more printable ASCII characters, none of <>&\"'")]
    Name(String),
    /// A cell's value is infinite, which VTK does not read back as written (ADR-0018).
    #[error("the cell at {origin:?} holds an infinite value, which VTK reads back wrongly")]
    Infinite {
        /// The cell's lowest corner.
        origin: Coord,
    },
    /// A cell's corner was not among the shared points.
    #[error("a cell's corner is missing from the shared points")]
    Inconsistent,
    /// Writing failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
