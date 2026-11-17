//! The command line (ADR-0050): a command, its input file and its options, parsed with
//! `std` alone. An option or argument a command does not take is an error naming it, never
//! ignored (CODING.md Rule 21).

use std::ffi::OsString;
use std::path::PathBuf;

use brodo::archive::Encoding;
use brodo::slice::Axis;

use crate::error::MestoloError;

/// The most a file may hold, and make when read, where `--limit` is not given: 4 GiB.
pub const DEFAULT_LIMIT: u64 = 4 << 30;

/// The file read, and the limit it is read under.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Input {
    /// Its path.
    pub path: PathBuf,
    /// The most it may hold, and the most reading it may make, in bytes.
    pub limit: u64,
}

impl Input {
    /// The file at `path`, read under the default limit.
    pub fn new(path: PathBuf) -> Self {
        Self { path, limit: DEFAULT_LIMIT }
    }
}

/// The file written, and whether one already there is replaced.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Output {
    /// Its path.
    pub path: PathBuf,
    /// Whether a file already at `path` is replaced rather than refused.
    pub force: bool,
    /// Whether `convert` leaves out the grids its format cannot hold, naming them, rather
    /// than refusing them all; given by `--skip-unsupported`.
    pub skip_unsupported: bool,
    /// How an archive `convert` writes stores the leaf values of its grids of a floating
    /// value type, given by `--encoding`; a grid of another type, and an entry of bytes, is
    /// stored exact whatever it is. `Exact` where `--encoding` is not given.
    pub encoding: Encoding,
}

impl Output {
    /// The file at `path`, one already there replaced if `force`, no grid left out, an
    /// archive's values stored exact.
    pub fn new(path: PathBuf, force: bool) -> Self {
        Self { path, force, skip_unsupported: false, encoding: Encoding::Exact }
    }
}

/// The format `convert` writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Format {
    /// An OpenVDB file, of every grid or the one named.
    Vdb,
    /// An ASCII `.vtu` of one grid.
    Vtu,
    /// brodo's cache profile.
    Cache,
    /// brodo's archive profile.
    Archive,
}

/// What to do.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Command {
    /// Write the usage.
    Help,
    /// Write the usage of the command named, one `parse` knows: given by `help <command>`,
    /// or by `-h` or `--help` among a command's arguments before `--`.
    HelpOn(String),
    /// Write mestolo's version, given by `--version` or `-V`.
    Version,
    /// Write what a file holds.
    Info(Input),
    /// Draw a slice of a grid as PNG.
    Slice {
        /// The file read.
        input: Input,
        /// The grid's name, where the file holds more than one.
        grid: Option<String>,
        /// The axis the slice is perpendicular to.
        axis: Axis,
        /// The slice's voxel coordinate along it.
        at: i32,
        /// The PNG written.
        output: Output,
    },
    /// Write a file's grids as `.vdb`, a cache file or an archive, or one as `.vtu`.
    Convert {
        /// The file read.
        input: Input,
        /// The grid's name: the one grid converted.
        grid: Option<String>,
        /// The format written, by `--as`; with none, by the output's name, `.vdb` or `.vtu`.
        format: Option<Format>,
        /// The file written.
        output: Output,
    },
}

/// The commands that take a file, each its usage in `help <command>`.
pub(crate) const COMMANDS: [&str; 3] = ["info", "slice", "convert"];

/// An option: its name, a value it takes, none for a flag, and the commands that take it.
pub(crate) struct Opt {
    /// Its name, as given.
    pub(crate) name: &'static str,
    /// A value it takes, for its tests; none for a flag, which takes no value.
    pub(crate) example: Option<&'static str>,
    /// The commands that take it.
    pub(crate) commands: &'static [&'static str],
}

/// Every option, the one list `parse` takes them from and the usage is tested against.
pub(crate) const OPTIONS: [Opt; 9] = [
    Opt { name: "--limit", example: Some("1"), commands: &["info", "slice", "convert"] },
    Opt { name: "--grid", example: Some("g"), commands: &["slice", "convert"] },
    Opt { name: "--axis", example: Some("x"), commands: &["slice"] },
    Opt { name: "--at", example: Some("-3"), commands: &["slice"] },
    Opt { name: "-o", example: Some("o"), commands: &["slice", "convert"] },
    Opt { name: "--force", example: None, commands: &["slice", "convert"] },
    Opt { name: "--as", example: Some("archive"), commands: &["convert"] },
    Opt { name: "--skip-unsupported", example: None, commands: &["convert"] },
    Opt { name: "--encoding", example: Some("8bit"), commands: &["convert"] },
];

/// The options given, each at most once.
#[derive(Default)]
struct Given {
    input: Option<PathBuf>,
    grid: Option<String>,
    axis: Option<Axis>,
    at: Option<i32>,
    output: Option<PathBuf>,
    force: bool,
    skip_unsupported: bool,
    limit: Option<u64>,
    format: Option<Format>,
    encoding: Option<Encoding>,
}

/// The command the arguments after the program's name give; none is refused, as
/// `NoCommand`.
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, MestoloError> {
    let mut args = args.into_iter();
    let Some(name) = args.next() else {
        return Err(MestoloError::NoCommand);
    };
    let name = text(name, "the command")?;
    let args: Vec<OsString> = args.collect();
    match name.as_str() {
        "help" | "-h" | "--help" => return Ok(help(&args)),
        "--version" | "-V" => return Ok(Command::Version),
        n if COMMANDS.contains(&n) => {}
        _ => return Err(MestoloError::UnknownCommand(name)),
    }
    let mut before_end = args.iter().take_while(|a| *a != "--");
    if before_end.any(|a| a == "-h" || a == "--help") {
        return Ok(Command::HelpOn(name));
    }
    let given = options(args.into_iter(), &name)?;
    command(&name, given)
}

/// The command `name` of the options `given`, refusing what it needs and was not given and
/// options that do not go together.
fn command(name: &str, given: Given) -> Result<Command, MestoloError> {
    let input = Input {
        path: given.input.ok_or(MestoloError::Missing("the input file"))?,
        limit: given.limit.unwrap_or(DEFAULT_LIMIT),
    };
    if given.encoding.is_some() && given.format != Some(Format::Archive) {
        return Err(MestoloError::EncodingNotArchive);
    }
    let (skip_unsupported, force) = (given.skip_unsupported, given.force);
    let encoding = given.encoding.unwrap_or_default();
    let output = given.output.map(|path| Output { path, force, skip_unsupported, encoding });
    if name == "info" {
        return Ok(Command::Info(input));
    }
    let output = output.ok_or(MestoloError::Missing("-o"))?;
    if name == "convert" {
        return Ok(Command::Convert { input, grid: given.grid, format: given.format, output });
    }
    Ok(Command::Slice {
        input,
        grid: given.grid,
        axis: given.axis.ok_or(MestoloError::Missing("--axis"))?,
        at: given.at.ok_or(MestoloError::Missing("--at"))?,
        output,
    })
}

/// The command of `help`, `-h` or `--help`: the usage of the command its first argument names, or the whole usage
/// for none or another.
fn help(args: &[OsString]) -> Command {
    match args.first().and_then(|a| a.to_str()) {
        Some(name) if COMMANDS.contains(&name) => Command::HelpOn(name.to_owned()),
        _ => Command::Help,
    }
}

/// The options and input file of `command`, of the options `OPTIONS` gives it; after `--`,
/// every argument is the input.
fn options(mut args: impl Iterator<Item = OsString>, command: &str) -> Result<Given, MestoloError> {
    let mut given = Given::default();
    let mut ended = false;
    while let Some(arg) = args.next() {
        let flag = arg.to_str().filter(|a| !ended && a.starts_with('-') && *a != "-");
        let Some(flag) = flag.map(str::to_owned) else {
            if given.input.is_some() {
                return Err(MestoloError::Extra(arg.to_string_lossy().into_owned()));
            }
            given.input = Some(PathBuf::from(arg));
            continue;
        };
        if flag == "--" {
            ended = true;
            continue;
        }
        let opt = OPTIONS.iter().find(|o| o.name == flag && o.commands.contains(&command));
        let Some(opt) = opt else {
            return Err(MestoloError::UnknownOption(flag));
        };
        let value = match opt.example {
            Some(_) => Some(args.next().ok_or(MestoloError::Missing(opt.name))?),
            None => None,
        };
        set(&mut given, opt.name, value)?;
    }
    Ok(given)
}

/// Sets option `name` to `value`, none for a flag, refusing it twice; a name no arm takes
/// is refused as not an option.
fn set(given: &mut Given, name: &'static str, value: Option<OsString>) -> Result<(), MestoloError> {
    let twice = || MestoloError::Repeated(name.to_owned());
    let v = value.unwrap_or_default();
    match name {
        "--force" => flag(&mut given.force, twice),
        "--skip-unsupported" => flag(&mut given.skip_unsupported, twice),
        "-o" => once(&mut given.output, PathBuf::from(v), twice),
        "--grid" => once(&mut given.grid, text(v, name)?, twice),
        "--axis" => once(&mut given.axis, axis(&text(v, name)?)?, twice),
        "--at" => once(&mut given.at, number(v, name)?, twice),
        "--as" => once(&mut given.format, format(&text(v, name)?)?, twice),
        "--encoding" => once(&mut given.encoding, encoding(&text(v, name)?)?, twice),
        "--limit" => once(&mut given.limit, number(v, name)?, twice),
        _ => Err(MestoloError::UnknownOption(name.to_owned())),
    }
}

fn flag(slot: &mut bool, twice: impl Fn() -> MestoloError) -> Result<(), MestoloError> {
    if std::mem::replace(slot, true) { Err(twice()) } else { Ok(()) }
}

fn once<T>(
    slot: &mut Option<T>,
    v: T,
    twice: impl Fn() -> MestoloError,
) -> Result<(), MestoloError> {
    if slot.replace(v).is_some() { Err(twice()) } else { Ok(()) }
}

fn text(arg: OsString, what: &'static str) -> Result<String, MestoloError> {
    arg.into_string().map_err(|_| MestoloError::NotText(what))
}

fn number<T: std::str::FromStr>(arg: OsString, option: &'static str) -> Result<T, MestoloError> {
    let s = text(arg, option)?;
    s.parse().map_err(|_| MestoloError::BadValue { option, value: s })
}

fn format(s: &str) -> Result<Format, MestoloError> {
    match s {
        "vdb" => Ok(Format::Vdb),
        "vtu" => Ok(Format::Vtu),
        "cache" => Ok(Format::Cache),
        "archive" => Ok(Format::Archive),
        _ => Err(MestoloError::BadValue { option: "--as", value: s.to_owned() }),
    }
}

fn encoding(s: &str) -> Result<Encoding, MestoloError> {
    match s {
        "exact" => Ok(Encoding::Exact),
        "binary16" => Ok(Encoding::Binary16),
        "8bit" => Ok(Encoding::EightBit),
        _ => Err(MestoloError::BadValue { option: "--encoding", value: s.to_owned() }),
    }
}

fn axis(s: &str) -> Result<Axis, MestoloError> {
    match s {
        "x" => Ok(Axis::X),
        "y" => Ok(Axis::Y),
        "z" => Ok(Axis::Z),
        _ => Err(MestoloError::BadValue { option: "--axis", value: s.to_owned() }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each option of `OPTIONS` is taken by the commands it names, its value read, and
    /// refused given twice; by another command it is refused as not an option. An option a
    /// table names and `set` does not would be refused as not an option.
    #[test]
    fn every_option_is_taken_by_its_commands_and_no_other() {
        for command in COMMANDS {
            for opt in &OPTIONS {
                let once: Vec<&str> = [opt.name].into_iter().chain(opt.example).collect();
                let words = |times: usize| {
                    let mut words = vec!["f"];
                    (0..times).for_each(|_| words.extend(&once));
                    words.into_iter().map(OsString::from).collect::<Vec<_>>()
                };
                let given = options(words(1).into_iter(), command);
                let twice = options(words(2).into_iter(), command);
                let name = opt.name.to_owned();
                if opt.commands.contains(&command) {
                    assert!(given.is_ok(), "{command} {}: {:?}", opt.name, given.err());
                    assert!(
                        matches!(&twice, Err(MestoloError::Repeated(n)) if *n == name),
                        "{command} {}: {:?}",
                        opt.name,
                        twice.err()
                    );
                } else {
                    assert!(
                        matches!(&given, Err(MestoloError::UnknownOption(n)) if *n == name),
                        "{command} {}: {:?}",
                        opt.name,
                        given.err()
                    );
                }
            }
        }
    }
}
