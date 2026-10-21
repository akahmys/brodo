# ADR-0031: colino reads triangle meshes, not STEP

- **Status**: Accepted
- **Date**: 2026-10-22

## Context

ADR-0017 left STEP (ISO 10303) for later, after STL, OBJ and 3MF, and ROADMAP.md's "After
P2" listed STEP input. STEP holds boundary representations: NURBS surfaces bounded by
trimming curves. Reading it means evaluating and triangulating those surfaces, the work of
a CAD kernel, with the dependencies one brings. brodo is a data-structure part, and colino
converts triangle meshes; neither is placed to carry such a kernel.

Against the records this one touches:

- ADR-0017's Consequences, "STEP is later", is reversed here.
- ADR-0026, how meshes are read, holds nothing on STEP and needs no change.
- ADR-0030 gave `Mesh` a group per triangle, `groups` and `group_names`, through which a
  B-rep's faces or shells can arrive by name. It is accepted but not yet built (R11), so
  the path this record points to exists in the design, not yet in the code.
- ARCHITECTURE.md §5.1 names STL, OBJ and 3MF as the inputs and does not name STEP.

## Decision

The owner decided, on 2026-10-22:

- colino does not read STEP. Its input stays a triangle mesh: read from STL, OBJ or 3MF,
  or a `Mesh` its caller builds.
- A user working with B-reps reads STEP with its own B-rep kernel, triangulates, and hands
  colino the mesh. The names of faces or shells travel as the mesh's `groups` and
  `group_names`, which colino keeps as ADR-0030's groups; no new API is made for them.
- "STEP input" leaves ROADMAP.md's "After P2". ARCHITECTURE.md §5.1 and colino's README
  each say in a sentence that STEP is not read and how a B-rep's faces arrive as groups.

## Consequences

colino gains no dependency on a CAD kernel. A STEP model reaches it only through its
user's triangulation, at the user's chosen accuracy.
