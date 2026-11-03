# ADR-0050: `mestolo` is a command that looks inside brodo's files

- **Status**: Accepted
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
