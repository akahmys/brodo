# ADR-0018: Visualisation is checking output in brodo and a viewer in a third crate, `piatto`

- **Status**: Amended
- **Date**: 2026-10-17

## Context

ARCHITECTURE.md left visualisation and GUI out of scope. Fields were checked by numbers
only, and some defects are easier seen than computed: a sign flipped, a hole where a tile
should be, a seam between neighbouring leaves. A viewer that any user can run or embed
needs a permissive home, and brodo's repository is the one its users share. The frozen
layout was laid out in `u32` words on 32-byte boundaries so that a GPU could read it
(ADR-0016), which nothing had yet tried.

## Decision

- **Checking output, in `brodo`, behind cargo features.** A slice of a field along x, y or
  z written as PNG: a diverging colour map centred on zero with the zero contour for
  signed distances, or a sequential one for other scalars; the colour range given by the
  caller, never inferred, so that results do not drift; tiles, the background and
  inactive regions shown apart from values; the same input giving the same bytes. And a
  field written for ParaView in a VTK format, chosen by B-012 with its reason recorded.
- **A viewer, in a new crate `piatto`** ("plate") in this repository, after P1: it reads
  a cache-profile file by memory mapping and draws, in its first version only, an
  isosurface by ray marching (zero by default), a slice with its position moved by the
  user, and a choice among the file's grids. Its drawing is a library, which applications
  embed; its windowed program is a thin user of that library. It also checks that a
  `GridView` buffer, copied to the GPU as it is, can be read there; anything that cannot is
  recorded as a question on the frozen layout. Window, input and GUI parts are chosen by
  B-013.
- **Out of scope stays out:** an application's own interface (choosing faces, labelling,
  showing provenance) belongs to that application, not to brodo or `piatto`.
- **Sources:** VTK and ParaView were not added to AGENTS.md S1. VTK output is written from
  the published file-format documentation only, never from VTK's or ParaView's source.
  PNG is encoded by a permissively licensed, pure-Rust crate.
- **A procedure for agents:** a change to values or topology is also checked by writing a
  slice as PNG and looking at it, as TESTING.md T8 states.
- Every new dependency (a PNG encoder; for `piatto`, a GPU, window and GUI stack) is
  approved and measured against CODING.md Rules 12, 13 and 20 before it is added. If
  `piatto` needed `unsafe`, ADR-0008 would be amended first.

## Consequences

P1 gains the checking output and a golden slice test in its acceptance; `piatto` is not
part of P1's acceptance and waits while ParaView suffices. Writing `.vdb` stays after P2.
The crate list (ARCHITECTURE.md §2) and README name `piatto` before it exists; it joins
`LAYERS` in `scripts/audit/project.py` when it is created.

## Amendment

- **Date**: 2026-10-17

The spike `spikes/vtk/` answered B-012 (its `REPORT.md`). The published documentation
read gave ImageData and UnstructuredGrid with ASCII data, but neither the multiblock nor the
hierarchical-box structure, nor binary data's header; the human limited the candidates to
those documented, leaving binary for when its documentation is found. Both candidates read
back exactly in VTK. One UnstructuredGrid of VTK_VOXEL cells was chosen: one file, with
tiles as larger cells, against per-leaf ImageData files, 4 to 6 times smaller and 3 times
faster but one file per leaf and tile, 5631 for a sphere of radius 80, with nothing to tie
them. VTK_VOXEL is 11, from VTK's public Python API, and its points are written x fastest,
then y, then z, which reading the files back confirmed.

## Amendment

- **Date**: 2026-10-17

Writing the `.vtu` in `brodo` settled three details, the human choosing each. Points are
`Float64`, which holds every `i32` coordinate exactly where `Float32` loses them beyond
2^24; connectivity and offsets are `Int64`. VTK 9.6.2's XML reader, given ASCII `Float32`
and `Float64` arrays, read `NaN` and `inf` back as written but `-inf` as `+inf`, and
refused the whole file on `Infinity` or `1e999`; so a NaN is written as `NaN`, and an
infinity of either sign is refused before anything is written, keeping the rule
symmetric.

## Amendment

- **Date**: 2026-10-20

A user asked to look at velocity fields in ParaView (ROADMAP.md §4, R7). VTK's XML
file-format page gives a `DataArray` a `NumberOfComponents`, "the number of components
per value", and `CellData` a `Vectors` attribute naming "the active vectors array"; it
states neither a default nor the order of a value's components in ASCII. The human
decided:

- The sealed `VtkScalar` becomes `VtkValue`, with its number of components, 1 or 3. It is
  implemented for `f32`, `f64`, `u8`, `u32`, `[f32; 3]` and `[f64; 3]`, and `write_vtu`
  takes any of them.
- A field of three components is written as `NumberOfComponents="3"`, each value's
  components on one line, and named by `CellData`'s `Vectors` attribute; a scalar field
  keeps `Scalars`, and its bytes.
- One field per file, as before.
- A NaN component is written as `NaN`; a value with an infinite component is refused
  before anything is written.
- The components' order, not stated in the documentation, is checked by reading files
  back in VTK 9.6.2, as the voxel's point order was.
