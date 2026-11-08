# ADR-0050: `mestolo` is a command that looks inside brodo's files

- **Status**: Amended
- **Date**: 2026-11-03

## Context

Someone who receives a cache file, an archive or a `.vdb` and writes no Rust had no way to
see what it holds: brodo is a library, and the viewer `piatto` is held (ROADMAP.md §3,
B-021), so the command could use no GPU and no GUI (the owner, 2026-11-02). P8 lists the
command with three subcommands, `info`, `slice` and `convert` (ROADMAP.md §2), its ADR first.

The workspace had no application crate (`scripts/audit/project.py`'s `APPLICATIONS` was
empty). brodo's readers of the three files are fuzzed and take a limit, `vdb::read`'s
bounding what reading makes since ADR-0047's amendment of 2026-11-03. The name `mestolo`
was found unregistered on crates.io on 2026-11-03.

## Decision

The owner decided, on 2026-11-03, the first three points; the rest were proposed with
them and accepted as proposed.

- **Where it lives**: a crate of its own, `crates/mestolo`, an application crate in
  `APPLICATIONS`, above colino in `LAYERS`, so that it may print and needs no rule's
  exception that a library crate would. A library and a thin `main`: the tests call the
  library. A `[[bin]]` in brodo would have put printing into a library crate and the command
  into `cargo install brodo`; `xtask` is not published, and would not reach its user.
- **Arguments**: parsed with `std` alone, no dependency: three subcommands and a few
  options. A path is kept as an `OsString`, and an option or input it does not take is an
  error naming it (CODING.md Rule 21). `lexopt` and `clap` were the alternatives; each is a
  dependency to approve, and `clap` a large tree.
- **Output**: `info` writes text in a fixed form to standard output; `slice` and `convert`
  write to the path `-o` names, refusing a file already there unless `--force` is given,
  written to a hidden file beside it and renamed over it, as a cache file is.
- **Input**: a file's kind is told by its magic number, not its name: a cache file, an
  archive or a `.vdb`. A cache file is read with `CacheFile::read`, not mapped, since its
  caller cannot promise that a file received stays unchanged (ADR-0036). Every file is read
  through brodo's fuzzed readers, under a limit of 4 GiB that `--limit` changes. The crate
  parses no bytes itself, so no module of it is in `UNTRUSTED_INPUT`.
- **What it builds with**: brodo without its default features, with `archive`, `vdb`,
  `slice-png` and `vtk`; without `mmap`, and so without brodo's unsafe module.
- **Errors**: an enum of its own, with `thiserror`; no `anyhow`.
- **Tests**: golden files, each subcommand's output from committed inputs held byte for
  byte to a committed one; the binary itself run, by `CARGO_BIN_EXE_mestolo`, for its
  arguments, its exit status and its refusal to overwrite.

## Consequences

The workspace has a third crate, with a README, a PROVENANCE.md row for each source file
and a test list, as the audit asks of every crate. `convert` writes `.vdb` for `f32` and
`f64` grids only, as brodo's writer does, and refuses the others by name. Whether mestolo
is published to crates.io with brodo and colino, and at which version, is P9's (ROADMAP.md
§2).

The crate landed the same day: `scripts/audit/project.py` lists it in `LAYERS` and
`APPLICATIONS`, and the workspace's `brodo` dependency takes no default features, so that
mestolo leaves `mmap` out, colino asking for it as before.

## Amendment: what the limit bounds, strings escaped, and the voxels counted

- **Date**: 2026-11-03

The owner's review of the crate, before its first commit, found three things:

- **Strings from a file**: `info` quoted and escaped grid names, but wrote the unit and
  each metadata key and value as they were, so that a control character could move the
  terminal or make a line of the fixed form. Every string read from a file is now
  written quoted and escaped as Rust's `{:?}` escapes it; brodo's errors already wrote
  each such string so, and mestolo's do too.
- **What `--limit` bounds**: its usage said it bounded what reading makes, but a cache
  file's or an archive's grids were then thawed outside it. The spike
  `spikes/thawmemory/` measured thawing: a tile about 139 bytes, a leaf of `f32` about
  2 381, and a node about twice the bytes it takes in the file. The choice was to count
  what can be counted, rather than only say what is not: each grid's leaves and tiles are
  counted from its view before it is thawed, a leaf its values' bytes and 1 024 more, a
  tile 256, as ADR-0047 counts a `.vdb`'s, and more than `--limit` is refused. Nodes,
  which `GridView` does not count, are not, and make about twice the file's bytes; the
  file is within the limit too. A count of nodes from brodo's public API is a proposal
  for P9's review of it.
- **Active voxels**: the line counted the voxels tiles cover with those of leaves,
  without saying so. It is now `voxels: N, L in leaves and T in tiles`.

The same review's mutation run found the slice's size, its colour map and the extremes'
ties untested; the first two are functions of their own with unit tests, and of equal
extremes, 0 and −0, the first in leaf order, then tile order, is written.


## Amendment: what its library promises

- **Date**: 2026-11-05

A survey before 1.0 found every module of `mestolo`'s library public, so that publishing it
would make each a promise under semantic versioning, though they are public only for its
tests. The owner decided, on 2026-11-05, that its promises be `run`, `USAGE`, `args` and
`error`, and that `convert`, `draw`, `info`, `load` and `output` stay public but
`#[doc(hidden)]`, promising nothing, as its README says.

## Amendment: the gaps T14's survey found

- **Date**: 2026-11-06

T14's survey (2026-11-05), by running the program, found four gaps that 1.0 takes: a slice
at a plane beyond the grid drew an image of the background alone and succeeded;
`convert` to `.vdb` named only the first grid of a type `.vdb` does not hold; the program
run with no argument wrote the usage and exited 0; and `convert` wrote no cache file or
archive. The owner decided, on 2026-11-06, as proposed:

- A slice at a plane beyond the grid's leaves and tiles along its axis is refused, as
  `OutsideGrid`, naming the grid, the plane and the span, and the command fails; a plane
  at either end of the span is drawn.
- `convert` to `.vdb` refuses naming every grid of a type it does not hold, with its value
  type, in the file's order: `NotWritten` holds them all. Writing the others and skipping
  those stays a request a later MINOR may take (ROADMAP.md §4).
- No argument is a command line refused, `NoCommand`: the usage on standard error and
  status 2; `help` writes it to standard output and succeeds.
- `--as vdb|vtu|cache|archive` names the format `convert` writes, before the output's
  name, which with no `--as` gives `.vdb` or `.vtu` as before; a cache file and an
  archive have no name of their own, so `--as` names them. A cache file's or an archive's
  entries are copied as they were (`CacheEntry::copy`, `CacheEntry::copy_bytes`,
  ADR-0060), its grids and then its entries of bytes, not thawed, so that a Hermite set or
  an SDF passes whole; a `.vdb`'s grids are written as read, an SDF read as stating the
  convention among them (ADR-0047). `--grid` writes the one grid it names. An archive's
  grids are written exact; choosing a quantised encoding is a request a later MINOR may
  take (ROADMAP.md §4).

## Amendment: rounding off the command line

- **Date**: 2026-11-08

The owner decided, on 2026-11-08, as proposed for P10:

- `--version` or `-V` writes `mestolo` and its version; `help <command>`, and `-h` or
  `--help` among a command's arguments before `--`, write that command's usage. `help`
  with no command or another still writes the whole usage, so that no command line it
  took is refused.
- `convert --skip-unsupported` writes as `.vdb` the grids it holds and names each other
  with its value type, a conversion leaving none refused as without it. The flag is held
  by `Output`, which is `#[non_exhaustive]`, since a field added to `Command::Convert`
  would break the 1.0.0 tests that build it (TESTING.md V9). The grids left out are
  named on standard output, the one writer `run` has, not on standard error as first
  proposed.
- `info` showing Hermite data and an archive's encodings changes its output, which
  §6.3 promises and the 1.0.0 tests compare whole (V9), so it waits for the next MAJOR
  (ROADMAP.md §4).
