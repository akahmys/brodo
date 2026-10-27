# ADR-0032: Hermite data is a brodo format that any producer writes

- **Status**: Amended
- **Date**: 2026-10-22

## Context

ADR-0030 defined Hermite data (signs on a lattice, and on each edge whose ends differ a
crossing, a normal and a group) as colino's output. Producers other than colino, such as
one building from analytic shapes, can make the same data, and a reader building cut cells
wants to read it without knowing which producer made it. ADR-0030 mixed the two kinds of
rule: what the data means, and how colino computes it.

The code as it stood (2026-10-22):

- `colino::hermite` was not built; `Mesh`'s groups and the three readers' groups were.
- A cache file's grids were read back only as `GridView`s; no function turned a frozen grid
  back into a `Topology` and a `Field`.
- Frozen, one topology gave the same bytes in its node sections (root table, upper and
  lower nodes) whatever the value type; only the header and the value sections differed.
  Measured by freezing a topology of four leaves and a lower tile as `u8`, `u32` and
  `[f32; 3]` and comparing the buffers.
- ADR-0030 stored each crossing's group as a `u32` grid, and the group names nowhere in
  the file.
- ADR-0030's amendment of 2026-10-22 put the ten grid names in colino, through
  `Hermite::entries`.

Hermite data is a geometric representation (Ju et al. 2002), not a solver's concept, so a
format for it in brodo keeps to AGENTS.md S4; cut cells and dual contouring stay outside
brodo (ARCHITECTURE.md §1).

## Decision

The owner decided, on 2026-10-22, the frame below, then chose among the options set out
for the points it left open.

**Where each rule lives**

| The format (brodo's documents and public API) | colino's choices as a producer (colino) |
| :--- | :--- |
| Point v lies at the corner of voxel v, the image of v under the grid's transform: the sampling lattice of an SDF on the same topology and metadata, shifted by −h/2 | The topology is its SDF conversion's, whose band puts every crossed edge's owner in a leaf |
| Point v owns its +x, +y and +z edges | Signs from the winding number, at the conversion's accuracy |
| Sign 1 inside, 0 outside; a tile's sign is its value, the background's is 0 | A point at distance zero from its nearest triangle is inside, its crossing at fraction 0 or 1 |
| An edge has a crossing exactly when the stored signs of its ends differ, those of tiles and background included; an edge whose owner is not a leaf point has none | Woop et al.'s watertight test against every triangle with area; zero-area triangles never crossed |
| Fraction: the crossing's place from the edge's lower end, in [0, 1] | The hit nearest the inside end; on a tie, the lowest triangle index |
| Normal: a unit vector; its component along the edge, where not zero, points from the inside end to the outside end | The crossed triangle's normal, flipped to point outward; kept as it is where perpendicular to the edge |
| Group: a `u32`, with a table naming each id, group 0 named `""` | The crossed triangle's group |
| An edge with no crossing, and every tile: fraction −1, normal zero, group 0 | No triangle hit: the triangle nearest the edge, at the edge's nearest point, and the count of such edges |
| Types: `u8` sign; per axis `f32` fraction, `[f32; 3]` normal, `u32` group | Values found in `f64`, stored in `f32`; the same bytes at any thread count |

**API**

- A type holding the ten fields as one set on one topology, with a function to make it,
  one to read it from a cache file, and a check of the format. The check refuses a set
  whose signs and crossings disagree, a fraction out of range, a normal not of unit length
  within a stated tolerance or pointing inward, a group with no name, a value other than
  the no-crossing one on a tile or on an edge without a crossing, or fields on different
  topologies.
- colino builds its output with this type. ADR-0030's acceptance tests hold it; colino's
  Hermite output was not built, so no bytes existed to keep.

**The points the frame left open**

- **a. Where the type and its check live**: a module of `brodo`. The check that ten frozen
  grids share a topology compares their node sections, which only `brodo` can see, and
  `brodo` already holds the file profiles and their versions (TESTING.md V6). A crate of
  its own was set aside: it would have needed a public brodo API comparing frozen
  topologies, besides a crate's README and audit settings.
- **b. How a file tells a set's grids apart**: keys in `GridMeta::extra`. `hermite.set`
  holds the set's name, and `hermite.field` which of the ten the grid is: `sign`, then
  `x.fraction`, `x.normal`, `x.group`, and the same for `y` and `z`. `GridMeta::name` is
  left to the producer, and one file can hold several sets. The group names go in the sign
  grid's `extra` as `hermite.group.<id>`, group 0 omitted. Fixed grid names, one set per
  file, and names prefixed by the set's were set aside.
- **c. A producer's optional information**, such as edges no triangle hit, is not in the
  format: the producer reports it beside the set, as colino's `unhit` count. Keys outside
  `hermite.` stay free. Keys under a producer's prefix and an eleventh grid of per-edge
  flags were set aside.
- **d. The format's version**: a key `hermite.version` in each grid's `extra`, `1` now. A
  reader accepts its own version only, as the cache reader does (ADR-0016), so that several
  crossings per edge or an `f64` table come as a new version. No version until a second one
  was set aside: a file written now stays recognisable after the change.
- **e. The form a set is read in**: ten `GridView`s, checked, without a copy, as the cache
  profile reads. A function turning a frozen grid back into a `Topology` and `Field`s waits
  for a user's request.

## Consequences

- ADR-0030 is amended to point here for the format and keeps colino's choices; its
  amendment's `Hermite::entries` and fixed names give way to b.
- ARCHITECTURE.md gains the format in §4, beside the file profiles, and §5.1 says colino
  produces it; §1's table names a producer of Hermite data and a reader building cut
  cells.
- The check gets one test per violation, each set breaking one rule and refused for it
  (TESTING.md T3), and a Format-tier test against a committed file.
- R11's fuzz round and `colino::intersect` do not wait for this record; `colino::hermite`
  does.

## Amendment

- **Date**: 2026-10-22

Three details of the format were left to its implementation. The owner decided:

- The group table's ids run from 1 with no gap; each name is non-empty and differs from
  the others, since a group is known by its name (ADR-0030).
- A set's ten grids agree in voxel size, unit and transform; their names, and keys outside
  `hermite.`, are free.
- A normal is of unit length when its length differs from 1 by no more than 1e-6. A unit
  `f64` vector rounded to `f32` errs by about 1e-7.

## Amendment: versions read, and thawing

- **Date**: 2026-10-27

P5's review found two decisions of this record overtaken. d's reader accepting "its own
version only" gave way to ADR-0038: a release that raises a version still reads the one
before it; every format is at version 1, so nothing read changed. And e's function turning
a frozen grid back into a topology and fields, which waited for a user's request, was
asked for (R12) and built as `GridView::thaw` and `thaw_on` (ADR-0042).
