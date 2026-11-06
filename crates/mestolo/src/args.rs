//! The command line (ADR-0050): a command, its input file and its options, parsed with
//! `std` alone. An option or argument a command does not take is an error naming it, never
//! ignored (CODING.md Rule 21).

use std::ffi::OsString;
use std::path::PathBuf;

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
}

impl Output {
    /// The file at `path`, one already there replaced if `force`.
    pub fn new(path: PathBuf, force: bool) -> Self {
        Self { path, force }
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

/// The options given, each at most once.
#[derive(Default)]
struct Given {
    input: Option<PathBuf>,
    grid: Option<String>,
    axis: Option<Axis>,
    at: Option<i32>,
    output: Option<PathBuf>,
    force: bool,
    limit: Option<u64>,
    format: Option<Format>,
}

/// The command the arguments after the program's name give; none is refused, as
/// `NoCommand`.
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, MestoloError> {
    let mut args = args.into_iter();
    let Some(name) = args.next() else {
        return Err(MestoloError::NoCommand);
    };
    let name = text(name, "the command")?;
    let takes: &[&str] = match name.as_str() {
        "help" | "-h" | "--help" => return Ok(Command::Help),
        "info" => &["--limit"],
        "slice" => &["--limit", "--grid", "--axis", "--at", "-o", "--force"],
        "convert" => &["--limit", "--grid", "--as", "-o", "--force"],
        _ => return Err(MestoloError::UnknownCommand(name)),
    };
    let given = options(args, takes)?;
    let input = Input {
        path: given.input.ok_or(MestoloError::Missing("the input file"))?,
        limit: given.limit.unwrap_or(DEFAULT_LIMIT),
    };
    let output = given.output.map(|path| Output { path, force: given.force });
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

/// The options and input file, of the options `takes` lists; after `--`, every argument
/// is the input.
fn options(
    mut args: impl Iterator<Item = OsString>,
    takes: &[&str],
) -> Result<Given, MestoloError> {
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
        if !takes.contains(&flag.as_str()) {
            return Err(MestoloError::UnknownOption(flag));
        }
        set(&mut given, &flag, &mut args)?;
    }
    Ok(given)
}

/// Sets `flag`'s value, taken from `args` where it has one, refusing it twice.
fn set(
    given: &mut Given,
    flag: &str,
    args: &mut impl Iterator<Item = OsString>,
) -> Result<(), MestoloError> {
    let twice = || MestoloError::Repeated(flag.to_owned());
    match flag {
        "--force" if given.force => return Err(twice()),
        "--force" => given.force = true,
        "-o" => once(&mut given.output, PathBuf::from(value(args, "-o")?), twice)?,
        "--grid" => once(&mut given.grid, text(value(args, "--grid")?, "--grid")?, twice)?,
        "--axis" => once(&mut given.axis, axis(&text(value(args, "--axis")?, "--axis")?)?, twice)?,
        "--at" => once(&mut given.at, number(value(args, "--at")?, "--at")?, twice)?,
        "--as" => once(&mut given.format, format(&text(value(args, "--as")?, "--as")?)?, twice)?,
        _ => once(&mut given.limit, number(value(args, "--limit")?, "--limit")?, twice)?,
    }
    Ok(())
}

fn once<T>(
    slot: &mut Option<T>,
    v: T,
    twice: impl Fn() -> MestoloError,
) -> Result<(), MestoloError> {
    if slot.replace(v).is_some() { Err(twice()) } else { Ok(()) }
}

fn value(
    args: &mut impl Iterator<Item = OsString>,
    option: &'static str,
) -> Result<OsString, MestoloError> {
    args.next().ok_or(MestoloError::Missing(option))
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

fn axis(s: &str) -> Result<Axis, MestoloError> {
    match s {
        "x" => Ok(Axis::X),
        "y" => Ok(Axis::Y),
        "z" => Ok(Axis::Z),
        _ => Err(MestoloError::BadValue { option: "--axis", value: s.to_owned() }),
    }
}
