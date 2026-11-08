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

/// Each option's lines in the usage, written once for every text that lists it.
macro_rules! grid {
    () => {
        "  --grid <name>    the grid, where the file holds more than one\n"
    };
}
macro_rules! limit {
    () => {
        "  --limit <bytes>  the most the file may hold, and its leaves and tiles make when read
                   and thawed (default 4294967296)\n"
    };
}
macro_rules! skip {
    () => {
        "  --skip-unsupported  leave out, and name, the grids the format cannot hold\n"
    };
}
macro_rules! force {
    () => {
        "  --force          replace a file already at the output's path\n"
    };
}

/// The usage, as `help` writes it.
pub const USAGE: &str = concat!(
    "\
usage: mestolo <command> <file> [options]

commands:
  info <file>                          what a cache file, an archive or a .vdb holds
  slice <file> --axis x|y|z --at <n> -o <out.png>
                                       a plane of a scalar grid, as PNG
  convert <file> -o <out.vdb|out.vtu> [--as vdb|vtu|cache|archive]
                                       the grids as .vdb, a cache file or an archive,
                                       or one grid as .vtu
  help [<command>]                     this text, or a command's
  --version                            mestolo's version

options:
",
    grid!(),
    limit!(),
    force!(),
    skip!(),
    "  -h, --help       the command's usage
"
);

/// A command's usage, as `help <command>` and the command's `-h` or `--help` write it.
fn usage_of(command: &str) -> Option<&'static str> {
    Some(match command {
        "info" => concat!(
            "usage: mestolo info <file> [--limit <bytes>]\n\n",
            "Writes what a cache file, an archive or a .vdb holds.\n\noptions:\n",
            limit!()
        ),
        "slice" => concat!(
            "usage: mestolo slice <file> --axis x|y|z --at <n> -o <out.png> [options]\n\n",
            "Draws the plane at voxel <n> along the axis of a scalar grid, as PNG.\n\noptions:\n",
            grid!(),
            limit!(),
            force!()
        ),
        "convert" => concat!(
            "usage: mestolo convert <file> -o <out> [--as vdb|vtu|cache|archive] [options]\n\n",
            "Writes the grids as .vdb, a cache file or an archive, or one grid as .vtu,\n",
            "in the format --as names or else by the output's name.\n\noptions:\n",
            grid!(),
            limit!(),
            force!(),
            skip!()
        ),
        _ => return None,
    })
}

/// Runs `command`: `info`'s text, the usage, a command's usage and the version are written
/// to `out`, and the grids `convert` leaves out, a slice or a
/// conversion to its output file, a file already there refused before the input is read.
pub fn run(command: &Command, out: &mut impl Write) -> Result<(), MestoloError> {
    match command {
        Command::Help => Ok(out.write_all(USAGE.as_bytes())?),
        Command::HelpOn(name) => Ok(out.write_all(usage_of(name).unwrap_or(USAGE).as_bytes())?),
        Command::Version => Ok(writeln!(out, "mestolo {}", env!("CARGO_PKG_VERSION"))?),
        Command::Info(input) => info::write(&load::load(input)?, out),
        Command::Slice { input, grid, axis, at, output } => {
            output::check(output)?;
            let loaded = load::load(input)?;
            draw::slice(loaded.grid(grid.as_deref())?, *axis, *at, &output.path)
        }
        Command::Convert { input, grid, format, output } => {
            output::check(output)?;
            convert::convert(input, grid.as_deref(), *format, output, out)
        }
    }
}
