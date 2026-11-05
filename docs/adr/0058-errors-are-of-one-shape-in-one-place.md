# ADR-0058: Errors are of one shape, in one place, and built with what gives them

- **Status**: Accepted
- **Date**: 2026-11-05

## Context

A survey of designs grown by accretion before 1.0 (ROADMAP.md P9) found brodo's errors
uneven. A version not read was told in four shapes: `FormatError::UnsupportedVersion`,
`CacheError::UnsupportedVersion`, `HermiteError::Version(String)` and
`VdbError::Version(u32)`, whose message also listed the versions read, a fact of
docs/formats.md §9.7. `GridError` and `AnalysisError` were defined in `grid` and
`analysis`, every other error in `error`. `SliceError`, `VdbError` and `VtkError` were
built without the feature that gives them, as were `CacheError`'s variants that only an
archive gives, while some of `SliceError`'s variants were gated. `VdbError`'s description
named writing alone, though the reader gives it too. `euler`, an analysis returning
`AnalysisError`, was a module of its own. And brodo and colino each have an error named
`HermiteError`.

## Decision

The owner decided, on 2026-11-05, as proposed:

- A version not read is `UnsupportedVersion`, holding the version as the file states it, a
  `u32` or, for Hermite data, whose version is a metadata value, a `String`, and saying
  "version {0} is not read by this build". The versions read are stated by docs/formats.md
  alone.
- Every public error of brodo is defined in `error`; `GridError` and `AnalysisError` are
  reached at `brodo::error`, not re-exported where they were.
- An error, or a variant, that only a feature's code gives is built with that feature:
  `SliceError` with `slice-png`, `VdbError` with `vdb`, `VtkError` with `vtk`, and
  `CacheError`'s `Decompress`, `Chunks`, `UnknownEncoding`, `NotQuantised` and
  `Unrepresentable` with `archive`. Every such enum is `#[non_exhaustive]`, so a feature
  turned on adds variants without breaking a `match`.
- `VdbError` is described as why grids could not be read or written.
- `euler` is `analysis::euler`, its source a private submodule of `analysis`.
- The two `HermiteError`s keep their names: colino names each error after the function
  that gives it, as `ConvertError` and `ReportError`, and its `HermiteError` says that it
  holds brodo's as `Format`. brodo's does not name colino's, knowing nothing of its users
  (AGENTS.md S4).
- An error's payload naming a part, a `&'static str`, stays one: what it says may change
  without breaking a user.

## Consequences

Callers matching `HermiteError::Version` or `VdbError::Version`, or naming
`brodo::grid::GridError`, `brodo::analysis::AnalysisError` or `brodo::euler::euler`,
change; CHANGELOG.md lists them. Code naming `SliceError`, `VdbError`, `VtkError` or an
archive's variant of `CacheError` turns on the feature that gives it. The messages of the
version errors change. The bytes are unchanged.
