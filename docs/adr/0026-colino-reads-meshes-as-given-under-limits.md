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

## Amendment

- **Date**: 2026-10-19 (second)

Before 3MF was read, the human approved its dependencies and decided how it is read.

- Dependencies, measured in a scratch crate: `zip` 8.6.0 failed Rule 13 on two advisories
  (RUSTSEC-2026-0368 and -0369, links followed on extraction). `zip` 9.0.5, deflate only,
  passed with 16 crates, nix and libc among them on macOS. `rawzip` 0.5.2, which reads
  the ZIP structure and leaves expansion to its caller, with `miniz_oxide` 0.9.1 for
  deflate and `quick-xml` 0.42.0, passed with 5 crates, `memchr` already present, the
  other four `#![forbid(unsafe_code)]`; the human chose them. `lo_zip` was not taken: it
  pulls an office suite's core, image and font crates, and states nothing of hostile
  input. Of XML readers fed eight hostile or broken inputs, `roxmltree` refused all, `xml`
  dropped an external entity silently, and `quick-xml` refused malformed tags and
  attributes but left the rest to its caller; on a 136 MB model `roxmltree` peaked at
  1004 MB and `quick-xml` at 140 MB. `quick-xml` was chosen, the reader refusing what it
  leaves: a document type, an undeclared prefix, an entity beyond XML's predefined ones
  and character references, a second root, an element left open, and text outside the
  root.
- `Limits` gains `bytes`, the most a file holds or a package's part expands to; a
  deflated part is expanded under it.
- The StartPart relationship names the model part, which must be a plain path. Part
  names are compared without case.
- Every build item is returned, its object placed by the item's transform after its
  components', the vertex a row vector on the left; a transform with a negative
  determinant turns triangles over. Support objects are skipped; a surface or `other`
  object in the build is refused; objects no item reaches are not returned. Placements
  count against the triangle limit, counted before expansion, so that components of
  empty meshes cannot make it endless.
- A model requiring an extension is refused; elements of other namespaces are skipped
  whole, attributes with a prefix ignored; an element of the core namespace the format
  does not define, or out of its place, is an error. The unit defaults to millimetres.
