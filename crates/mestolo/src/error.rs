//! Why a command could not be run: its command line refused, its input not read, or its
//! output not written.

use brodo::field::ValueType;
use brodo::slice::Axis;
use std::path::PathBuf;

use brodo::error::{
    CacheError, FieldError, FormatError, SliceError, TreeError, VdbError, VtkError,
};

/// Why a command could not be run.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MestoloError {
    /// No argument is given, so no command.
    #[error("no command is given")]
    NoCommand,
    /// The first argument names no command.
    #[error("there is no command {0:?}")]
    UnknownCommand(String),
    /// An option the command does not take.
    #[error("{0:?} is not an option of this command")]
    UnknownOption(String),
    /// An option given twice.
    #[error("{0:?} is given twice")]
    Repeated(String),
    /// What the command needs and was not given, named.
    #[error("{0} is missing")]
    Missing(&'static str),
    /// An argument beyond the input file.
    #[error("{0:?} is an argument too many: the input file is given once")]
    Extra(String),
    /// An option's value it does not take.
    #[error("{option} does not take {value:?}")]
    BadValue {
        /// The option.
        option: &'static str,
        /// The value given.
        value: String,
    },
    /// An argument that is not UTF-8 where only text is taken, named.
    #[error("{0} is not UTF-8")]
    NotText(&'static str),
    /// The input's first bytes are those of no file mestolo reads.
    #[error("{0:?} is not a cache file, an archive or a .vdb")]
    UnknownKind(PathBuf),
    /// The input is a bare frozen grid, which brodo gives no public way to read.
    #[error("{0:?} is a bare frozen grid, which is not read")]
    BareGrid(PathBuf),
    /// Thawing a cache file's or an archive's grids would make more than the limit, counted
    /// as ADR-0050 states.
    #[error("thawing the grids would make {counted} bytes or more, more than the limit of {limit}")]
    TooMuchToThaw {
        /// What was counted when the count passed the limit.
        counted: u64,
        /// The limit.
        limit: u64,
    },
    /// A grid's values are of a type this build of mestolo does not know, one brodo added
    /// after it.
    #[error("values of type {0} are not read by this build")]
    ValueType(ValueType),
    /// The file holds no grid of the name given.
    #[error("the file holds no grid named {0:?}")]
    NoGrid(String),
    /// The file holds a number of grids other than one, and none was named.
    #[error("the file holds {0} grids: name one with --grid")]
    WhichGrid(usize),
    /// The grid has no leaf or tile, so there is nothing to draw.
    #[error("grid {0:?} has no leaf or tile to draw")]
    EmptyGrid(String),
    /// A slice's plane lies beyond the grid's leaves and tiles along its axis.
    #[error("grid {grid:?} spans {min} to {max} along {}, so there is no plane at {at}", axis_name(*.axis))]
    OutsideGrid {
        /// The grid's name.
        grid: String,
        /// The axis the slice is perpendicular to.
        axis: Axis,
        /// The plane asked for.
        at: i32,
        /// The least voxel coordinate of the grid's leaves and tiles along the axis.
        min: i32,
        /// The greatest.
        max: i32,
    },
    /// A slice of a grid whose values are not scalars.
    #[error("a slice of {0} values is not drawn: they are not scalars")]
    NotScalar(ValueType),
    /// A slice's image would be larger than mestolo draws.
    #[error("the slice would be {width} × {height} pixels, more than {most}")]
    ImageTooLarge {
        /// Its width.
        width: i64,
        /// Its height.
        height: i64,
        /// The most pixels drawn.
        most: i64,
    },
    /// The output's name ends in no format mestolo writes, and `--as` names none.
    #[error("{0:?} ends in neither .vdb nor .vtu: name the format with --as")]
    OutputKind(PathBuf),
    /// The grids whose value types `.vdb` does not hold, each its name and value type, in
    /// the file's order.
    #[error(".vdb does not hold the values of {}", not_written(.0))]
    NotWritten(Vec<(String, ValueType)>),
    /// A file is at the output's path, and `--force` was not given.
    #[error("{0:?} exists: give --force to replace it")]
    Exists(PathBuf),
    /// Reading or writing a file failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// A cache file or an archive was refused.
    #[error(transparent)]
    Cache(#[from] CacheError),
    /// A grid's frozen buffer was refused.
    #[error(transparent)]
    Format(#[from] FormatError),
    /// A `.vdb` was refused, or could not be written.
    #[error(transparent)]
    Vdb(#[from] VdbError),
    /// A field did not belong to its topology.
    #[error(transparent)]
    Field(#[from] FieldError),
    /// A topology refused a lookup.
    #[error(transparent)]
    Tree(#[from] TreeError),
    /// A slice could not be drawn or encoded.
    #[error(transparent)]
    Slice(#[from] SliceError),
    /// A `.vtu` could not be written.
    #[error(transparent)]
    Vtk(#[from] VtkError),
}

impl MestoloError {
    /// Whether the command line was refused, rather than the command failing.
    pub fn is_usage(&self) -> bool {
        matches!(
            self,
            Self::NoCommand
                | Self::UnknownCommand(_)
                | Self::UnknownOption(_)
                | Self::Repeated(_)
                | Self::Missing(_)
                | Self::Extra(_)
                | Self::BadValue { .. }
                | Self::NotText(_)
        )
    }
}

fn axis_name(axis: Axis) -> &'static str {
    match axis {
        Axis::X => "x",
        Axis::Y => "y",
        Axis::Z => "z",
    }
}

/// Each grid as `"name" (type)`, joined by commas.
fn not_written(grids: &[(String, ValueType)]) -> String {
    grids.iter().map(|(name, t)| format!("{name:?} ({t})")).collect::<Vec<_>>().join(", ")
}
