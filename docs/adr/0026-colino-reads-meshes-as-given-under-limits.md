# ADR-0026: colino reads meshes as given, under limits, and welds by exact position

- **Status**: Amended
- **Date**: 2026-10-19

## Context

P2's second item reads STL, OBJ and 3MF (ARCHITECTURE.md §5.1), files from outside that
may be damaged or hostile. §5.2's first layer checks the Euler characteristic against the
input's own V − E + F, which needs the mesh's connectivity; STL lists each triangle's
corners apart. The Library of Congress's descriptions of STL (fdd000505, fdd000506) leave
open how numbers are written, whether case matters, and binary files that begin with
`solid`.

## Decision

The human decided, on 2026-10-19:

- A `Mesh` holds `f64` vertices, `u32` triangles indexing them and the unit the file
  states, uninterpreted, or none. Nothing is repaired: badly oriented, holed and
  degenerate triangles are kept (ADR-0017).
- `Mesh::welded` joins vertices whose coordinates are equal, `-0.0` and `0.0` as one, never
  within a tolerance, and refuses a triangle naming a vertex the mesh lacks.
- Every reader takes `Limits` on triangles and vertices from its caller and fails beyond
  them; a count a file states is checked against what the file holds before anything is
  allocated for it.
- STL: binary when the length is 84 + 50 × the stated count, even when it begins with
  `solid`; otherwise ASCII, which must begin with `solid`. ASCII words are compared
  without case, numbers read as Rust reads an `f64` and finite, a loop of other than
  three vertices refused. Normals and attribute bytes are not used.
- OBJ reads `v` and `f`, negative indices included, polygons split into fans; the
  statements it has no use for are skipped by name, and any other is an error.
- 3MF needs ZIP and XML; the crates for them are checked against Rules 12, 13 and 20 and
  approved before its reader is written.
- `colino::mesh` holds the type and welding; `colino::stl`, `colino::obj` and
  `colino::threemf` read, each in `UNTRUSTED_INPUT` (ADR-0009) with a fuzz target.

## Consequences

Each reader comes in a unit of its own, with its fuzz target. A mesh read from STL has
three vertices a triangle until it is welded.

## Amendment

- **Date**: 2026-10-19

Reading the Wavefront specification as published at paulbourke.net/dataformats/obj/, the
OBJ reader was settled so. A vertex has 3 numbers, 4 with the specification's weight, or
6 with three colour numbers, an extension exporters write that the specification does
not name; the weight and colour are not used, and any other count is an error. Of a face
only each reference's vertex number is read, whatever its form. Skipped by name: texture
and normal vertices, grouping, display and rendering attributes, points and lines, which
add no triangle. Refused: free-form geometry, which defines shape this reader does not
triangulate, and `call` and `csh`, which would read another file or run a command. A
positive number may name a vertex later in the file, checked once it is all read.
