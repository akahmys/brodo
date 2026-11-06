#![doc = include_str!("../README.md")]
//!
//! The design is in the repository's `ARCHITECTURE.md` §6.3 and ADR-0050.

use std::io::Write;

pub mod args;
pub mod error;

// The command's own parts, public for its tests and hidden: they promise nothing across
// versions (ADR-0050).
#[doc(hidden)]
pub mod convert;
#[doc(hidden)]
pub mod draw;
#[doc(hidden)]
pub mod info;
#[doc(hidden)]
pub mod load;
#[doc(hidden)]
pub mod output;

use args::Command;
use error::MestoloError;

/// The usage, as `help` writes it.
pub const USAGE: &str = "\
usage: mestolo <command> <file> [options]

commands:
  info <file>                          what a cache file, an archive or a .vdb holds
  slice <file> --axis x|y|z --at <n> -o <out.png>
                                       a plane of a scalar grid, as PNG
  convert <file> -o <out.vdb|out.vtu> [--as vdb|vtu|cache|archive]
                                       the grids as .vdb, a cache file or an archive,
                                       or one grid as .vtu
  help                                 this text

options:
  --grid <name>    the grid, where the file holds more than one
  --limit <bytes>  the most the file may hold, and its leaves and tiles make when read
                   and thawed (default 4294967296)
  --force          replace a file already at the output's path
";

/// Runs `command`: `info`'s text and the usage are written to `out`, a slice or a
/// conversion to its output file, a file already there refused before the input is read.
pub fn run(command: &Command, out: &mut impl Write) -> Result<(), MestoloError> {
    match command {
        Command::Help => Ok(out.write_all(USAGE.as_bytes())?),
        Command::Info(input) => info::write(&load::load(input)?, out),
        Command::Slice { input, grid, axis, at, output } => {
            output::check(output)?;
            let loaded = load::load(input)?;
            draw::slice(loaded.grid(grid.as_deref())?, *axis, *at, &output.path)
        }
        Command::Convert { input, grid, format, output } => {
            output::check(output)?;
            convert::convert(input, grid.as_deref(), *format, &output.path)
        }
    }
}
