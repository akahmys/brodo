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
macro_rules! encoding {
    () => {
        "  --encoding exact|binary16|8bit
                   with --as archive, how the values of floating grids are stored
                   (default exact); other grids are stored exact\n"
    };
}
macro_rules! drawing {
    () => {
        "  --component x|y|z|magnitude
                   of a vector grid, the component drawn
  --range <min>,<max>
                   the values drawn darkest and lightest (default the grid's least and
                   greatest; a level set's diverge at zero)
  --region <u0>,<v0>,<u1>,<v1>
                   the part of the plane drawn, in its two other voxel coordinates,
                   inclusive (default the grid's bounds)\n"
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
    encoding!(),
    drawing!(),
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
            "Draws the plane at voxel <n> along the axis of a scalar grid, or of a component\n",
            "of a vector grid, as PNG.\n\noptions:\n",
            grid!(),
            limit!(),
            force!(),
            drawing!()
        ),
        "convert" => concat!(
            "usage: mestolo convert <file> -o <out> [--as vdb|vtu|cache|archive] [options]\n\n",
            "Writes the grids as .vdb, a cache file or an archive, or one grid as .vtu,\n",
            "in the format --as names or else by the output's name.\n\noptions:\n",
            grid!(),
            limit!(),
            force!(),
            skip!(),
            encoding!()
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
            draw::slice(loaded.grid(grid.as_deref())?, *axis, *at, &output.drawing, &output.path)
        }
        Command::Convert { input, grid, format, output } => {
            output::check(output)?;
            convert::convert(input, grid.as_deref(), *format, output, out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use args::{COMMANDS, OPTIONS};

    /// A command's usage names the options it takes and no other; the whole usage names
    /// every option.
    #[test]
    fn each_usage_names_the_options_its_command_takes() {
        for command in COMMANDS {
            let usage = usage_of(command).unwrap_or_default();
            assert!(usage.starts_with(&format!("usage: mestolo {command} ")), "{usage}");
            for opt in &OPTIONS {
                let named = usage.contains(&format!(" {} ", opt.name));
                assert_eq!(named, opt.commands.contains(&command), "{command} {}", opt.name);
                assert!(USAGE.contains(&format!(" {} ", opt.name)), "{}", opt.name);
            }
        }
    }
}
