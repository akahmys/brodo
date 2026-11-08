# ADR-0018: Visualisation is checking output in brodo and a viewer in a third crate, `piatto`

- **Status**: Amended
- **Date**: 2026-10-17

Condensed on 2026-11-08 (ADR-0001): each decision is kept with its reason and what it set
aside; the text before is in git, this file as of `53f69d5`.

## Context

ARCHITECTURE.md left visualisation and GUI out of scope. Fields were checked by numbers
only, and some defects are easier seen than computed: a sign flipped, a hole where a tile
should be, a seam between neighbouring leaves. The frozen layout was laid out in `u32`
words on 32-byte boundaries so that a GPU could read it (ADR-0016), which nothing had yet
tried.

## Decision

The human decided, on 2026-10-17 unless dated otherwise.

- **A slice as PNG, in `brodo`**, behind a feature: a field cut along x, y or z, a
  diverging colour map centred on zero with the zero contour for signed distances, or a
  sequential one for other scalars; the colour range given by the caller, never inferred,
  so that results do not drift; tiles, the background and inactive regions shown apart
  from values; the same input giving the same bytes. The contour is drawn across leaves
  and tiles alike, a tile's voxel nearer zero than its neighbour of the other sign on it
  (2026-11-02: it had broken where a leaf met a tile). An agent checks a change to values
  or topology by writing a slice and looking at it (TESTING.md T8).
- **A field for ParaView**, behind a feature, as one VTK XML UnstructuredGrid of
  `VTK_VOXEL` cells (B-012, `spikes/vtk/`): one file, tiles as larger cells. Per-leaf
  ImageData files were 4 to 6 times smaller and 3 times faster but one file per leaf and
  tile, 5631 for one sphere, with nothing to tie them; multiblock, hierarchical boxes and
  binary data were not in the documentation read. Points are `Float64`, which holds every
  `i32` coordinate where `Float32` loses them beyond 2²⁴, written x fastest; connectivity
  and offsets `Int64`. A NaN is written `NaN`, and a value with an infinite component is
  refused before anything is written: VTK 9.6.2 read `-inf` back as `+inf` and refused
  `Infinity`. A value of three components is one line, `NumberOfComponents="3"`, named by
  `Vectors` (2026-10-20, R7), a scalar by `Scalars`; one field per file. What the
  documentation leaves unstated, the point and component order, is checked by reading
  files back in VTK.
- **Sources.** VTK and ParaView are not in AGENTS.md S1, but VTK output is written from
  the published file-format documentation only, never from VTK's or ParaView's source.
  PNG is encoded by a permissively licensed, pure-Rust crate.
- **A viewer, `piatto`, is not built for now** (2026-10-24). Its design is kept in
  ARCHITECTURE.md §6.2: a library applications embed and a thin windowed program, reading
  a cache file by mapping and drawing an isosurface by ray marching, a slice the user
  moves and a choice of grid, and checking that a `GridView` buffer copied to the GPU as
  it is can be read there. By P3 no user had asked for one; the slice, ParaView and `.vdb`
  showed what was needed, and a GPU, window and GUI stack would bring heavy dependencies,
  perhaps `unsafe`, and `f64` values a GPU seldom reads. It is built when a user asks for
  a viewer or drawing to embed, or when grids too large for VTK and `.vdb` must be looked
  at. Its window, input and GUI parts wait with it (B-013).
- **Out of scope stays out**: an application's own interface, choosing faces, labelling,
  showing provenance, belongs to that application.
- Every new dependency is approved and measured against CODING.md Rules 12, 13 and 20
  before it is added.

## Consequences

The crate list (ARCHITECTURE.md §2) and README name `piatto` before it exists; it joins
`LAYERS` in `scripts/audit/project.py` when it is created.
