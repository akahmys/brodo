# brodo's file formats

The bytes of brodo's files, for a reader or writer in any language (ADR-0037). Where this
document and the code differ, this document is right and the code is corrected; a
difference found is a defect of the format's version. The design behind the formats is in
ARCHITECTURE.md §4.6, §4.7 and §4.9.

`scripts/audit/formats.py` is a reader written from this document alone. The gate runs it on
the golden files and on damaged copies of them, through
`crates/brodo/tests/format_document.rs`, and fails where it reads a value other than the
crates read, or accepts what they refuse, or refuses what they accept.

## 1. Conventions

- Every number is little-endian. `u32` and `i32` are 4 bytes, `f32` 4, `f64` 8, IEEE 754.
- A **block** is 32 bytes. Offsets are in blocks from the start of the buffer or file, and
  every structure begins on a block boundary.
- A coordinate is three `i32`, x, y, z. A voxel v covers [v, v + 1) in index space.
- **Padding** and unused words are written as zero bytes.
- A string is a `u32` byte length, its UTF-8 bytes, and zero bytes up to a multiple of 4.

### 1.1 Value types

| Code | Type | Size, bytes |
| :--- | :--- | :--- |
| 1 | `f32` | 4 |
| 2 | `f64` | 8 |
| 3 | `u8` | 1 |
| 4 | `u32` | 4 |
| 5 | three `f32`, x, y, z | 12 |
| 6 | three `f64`, x, y, z | 24 |

Any other code is refused.

### 1.2 The tree

A grid is a tree of fixed shape over the voxels:

| Level | Region edge, voxels | Children | Child edge |
| :--- | :--- | :--- | :--- |
| root table | 4096 | one upper node or one root tile per region | |
| upper node | 4096 | 32³ places: a lower node or an upper tile | 128 |
| lower node | 128 | 16³ places: a leaf or a lower tile | 8 |
| leaf | 8 | 8³ values | 1 |

A region's **origin** is its smallest voxel; at a level of edge 2^k it is v with its k low
bits cleared in two's complement, each axis. A tile holds one value for its whole region.
A voxel in no leaf or tile holds the **background**.

The **place** of a child in a node with 2^b places a side, from the child's local
coordinates (x, y, z), each `(c >> child_edge_log2) & (2^b − 1)` of the child's origin c
taken as an unsigned 32-bit number: bit i of x, y and z goes to bit 3i, 3i + 1 and 3i + 2
of the place (Morton order, x lowest). b is 5 in an upper node, 4 in a lower node.

A leaf's 512 values are ordered x fastest: the value of voxel origin + (x, y, z) is at
index x + 8y + 64z.

The **Morton key** of an origin interleaves the bits of its three coordinates, each first
XORed with 0x80000000 so that order follows the signed value: bit i of x, y and z goes to
bit 3i, 3i + 1 and 3i + 2.

## 2. The frozen layout, version 1

One buffer holding one grid, read in place (ARCHITECTURE.md §4.6). A bare frozen buffer is
also a file of its own, as the golden files `frozen_v1_*.bin` are.

| Section | At | Entry size, bytes | Order |
| :--- | :--- | :--- | :--- |
| header | block 0 | 96 | |
| root table | its offset | 32 | strictly ascending by key, compared as (x, y, z), x first |
| upper nodes | its offset | 8224 | ascending Morton key of origin |
| lower nodes | its offset | 1056 | ascending Morton key of origin |
| leaf values | its offset | 512 × value size | the leaves' Morton order |
| tile values | its offset | value size | root tiles, then each upper node's, then each lower node's |

The writer places each section at the block after the previous one ends, the first at
block 3, and the buffer ends at the last section's block boundary.

### 2.1 Header, 96 bytes

| Byte | Field | Value |
| :--- | :--- | :--- |
| 0 | magic | the bytes `BRDG` |
| 4 | version | `u32` 1 |
| 8 | value type | `u32` code, §1.1 |
| 12 | value size | `u32` bytes, §1.1 |
| 16 | counts | 5 × `u32`: root entries, upper nodes, lower nodes, leaves, tiles |
| 36 | offsets | 5 × `u32`: the same sections' offsets, in blocks |
| 56 | blocks | `u32`: the buffer's length, in blocks |
| 60 | background | 24 bytes: the background value, zero beyond its size |
| 84 | padding | 12 bytes |

### 2.2 Root entry, 32 bytes

| Byte | Field | Value |
| :--- | :--- | :--- |
| 0 | key | 3 × `i32`: the region's origin, each a multiple of 4096 |
| 12 | kind | `u32`: 0 a root tile, 1 an upper node |
| 16 | index | `u32`: the tile's place among tile values, or the upper node's number |
| 20 | padding | 12 bytes |

Root tiles take tile places 0, 1, … in table order.

### 2.3 Upper node, 8224 bytes, and lower node, 1056 bytes

| Byte, upper | Byte, lower | Field |
| :--- | :--- | :--- |
| 0 | 0 | children: a mask of 32³ (upper) or 16³ (lower) bits, as `u32` words, place p at bit p mod 32 of word p div 32 |
| 4096 | 512 | tiles: a mask of the same size |
| 8192 | 1024 | first child: `u32` |
| 8196 | 1028 | first tile: `u32` |
| 8200 | 1032 | padding, 24 bytes |

The child at place p is number first child + the children bits set below p: a lower node
in an upper node, a leaf in a lower node. The tile at place p is at tile place first tile
+ the tiles bits set below p. A node's children are consecutive in place order; so are
its tiles. A node with none of either holds 0.

Tile places after the root tiles go to the upper nodes in node order, each node's tiles in
place order, then to the lower nodes likewise.

### 2.4 Finding a voxel

The root entry with the key of the voxel's 4096-region; none, the background. A root tile
gives its value. In the upper node, the voxel's place of edge 128: a tiles bit gives the
tile; no children bit, the background; else the lower node. There, the place of edge 8: a
children bit gives the leaf, a tiles bit the tile, neither the background.

### 2.5 What a reader checks

A reader refuses a buffer, before reading any value, unless:

1. its length is a multiple of 32 and at least 96, and, read in place, it begins at a
   32-byte boundary in memory; a reader that copies it aligns the copy;
2. the magic, version 1, and the value type and size are those the reader asks for;
3. blocks × 32 is the buffer's length;
4. each section, in the order of the table, starts at or after the end of the one before,
   the header ending at byte 96, and ends within the buffer, counts × entry size bytes;
5. root keys are strictly ascending and multiples of 4096, each kind 0 or 1, and each
   index less than the tile count (kind 0) or the upper node count (kind 1);
6. in every upper node, first child + its children bits ≤ the lower node count, first
   tile + its tiles bits ≤ the tile count, and no place has both bits; in every lower node
   the same, against the leaf count and the tile count.

Padding, the background's bytes beyond its size, the order of nodes, whether every node and
leaf is reached, and three more of the writer's rules, that a node with no child or tile
holds 0 for each first, that root tiles take tile places in table order, and that each
section starts at the block right after the one before, are not checked.

## 3. The cache profile, format version 1

Grids with their metadata in one file (ARCHITECTURE.md §4.7). Media type
`application/vnd.brodo.cache; version=1`, stated and not registered (ADR-0041).

| Part | At | Size |
| :--- | :--- | :--- |
| header | block 0 | 1 block |
| directory | block 1 | 1 block per grid |
| per grid, in directory order: metadata | the block after the previous part | its length, zero-padded to a block |
| then its frozen buffer, §2 | the block after its metadata | its blocks |

### 3.1 Header, one block

| Byte | Field | Value |
| :--- | :--- | :--- |
| 0 | magic | the bytes `BRDC` |
| 4 | version | `u32` 1 |
| 8 | grid count | `u32` |
| 12 | unused | 20 bytes, zero |

### 3.2 Directory entry, one block

| Byte | Field |
| :--- | :--- |
| 0 | metadata offset, `u32` blocks |
| 4 | metadata length, `u32` bytes |
| 8 | frozen buffer offset, `u32` blocks |
| 12 | frozen buffer length, `u32` blocks |
| 16 | value type, `u32` code, §1.1 |
| 20 | unused, 12 bytes, zero |

### 3.3 Metadata

In order: the name, a string; the unit, a string; the voxel size, 3 × `f64`; the transform
from voxel to world coordinates, 12 × `f64`, three rows of four, a point p mapping to row ·
(p, 1); the bounds kind, `u32`, 0 empty or 1 a box; 6 × `i32`, the box's smallest and
largest voxel, inclusive, x, y, z each, or zero for an empty one; the count of free pairs,
`u32`; each pair a key string and a value string, keys strictly ascending by their bytes.
The writer works the bounds out from the grid's leaves and tiles.

### 3.4 What a reader checks

A reader refuses a file, before answering anything, unless:

1. its length is a multiple of 32;
2. the magic and version 1, and the header's unused bytes zero;
3. for each grid: the directory's unused words zero; its metadata starting at the block
   right after the previous part, (count + 1) × 32 for the first, and its frozen buffer at
   the block right after the metadata, both ending within the file; the bytes between them
   zero;
4. its metadata: strings valid UTF-8 with zero padding; the bounds kind 0 or 1, an empty
   box's corners zero; keys strictly ascending; no byte left over; the unit not empty; the
   voxel size finite and positive; the transform finite;
5. its frozen buffer passes §2.5 with the directory's value type;
6. the last grid's buffer ends where the file ends.

The bounds are not checked against the grid.

## 4. Hermite data, version 1

A sign at each lattice point and, on each edge whose ends differ, where a surface crosses
it, its normal there and its group (ARCHITECTURE.md §4.9, ADR-0032). Media type
`application/vnd.brodo.hermite; version=1` for a cache file holding a set, stated and not
registered (ADR-0041); a reader checks it as a cache file first.

### 4.1 The set

Point v lies at the corner of voxel v, the image of v under the transform; it owns the
edges from v to v + x̂, v + ŷ and v + ẑ. A set is ten grids of a cache file on one tree:

| Field | Type | Holds |
| :--- | :--- | :--- |
| `sign` | `u8` | 1 inside, 0 outside |
| `x.fraction`, `y.fraction`, `z.fraction` | `f32` | the crossing's place from the edge's lower end |
| `x.normal`, `y.normal`, `z.normal` | three `f32` | the surface's unit normal at the crossing |
| `x.group`, `y.group`, `z.group` | `u32` | the crossing's group id |

Each grid's free pairs hold `hermite.version` = `1`, `hermite.set` = the set's name and
`hermite.field` = its field. The sign grid holds the group table: `hermite.group.<id>` =
the group's name, for ids 1, 2, … with no gap, the id in decimal with no leading zero;
group 0, named `""`, has no key. Keys beginning `hermite.` are the format's: a writer puts
no other, and a reader reads the group table from the sign grid alone and leaves every
other key unread. Other grids and other keys are free.

An edge with no crossing holds **no crossing**: fraction −1, normal (0, 0, 0), group 0, as
bits.

### 4.2 What a reader checks

A reader finds the grids whose `hermite.set` is the name asked for, and refuses the set
unless:

1. each has `hermite.version` 1 and a `hermite.field` of the table, each field once, all
   ten present, each of its type;
2. the ten agree in voxel size and transform, as bits, and unit;
3. the ten root tables, upper nodes and lower nodes are the same bytes, and the ten hold
   as many leaves and as many tiles;
4. the group table's keys are as §4.1 says, the names not empty and none twice;
5. the signs' background is 0, and the three axes' backgrounds hold no crossing;
6. every tile's sign is 0 or 1 and its crossings no crossing;
7. every leaf point's sign is 0 or 1;
8. for every tile, every point just outside each face, in no leaf, has the tile's sign;
   tiles and the background cover whole 8³ blocks, so one point of each 8 × 8 patch of a
   face stands for the patch;
9. for every leaf point v and axis a, with w = v + â and the signs where §2.4 finds them:
   if v and w have the same sign, the edge holds no crossing; otherwise its fraction lies
   in [0, 1], its normal's length, worked in `f64`, is within 10⁻⁶ of 1, its component
   along a is not negative when v is inside and not positive when v is outside, and its
   group is below the table's length, or is 0 when the table is empty;
10. for every leaf point v and axis a, the point u = v − â, when in no leaf, has v's sign;
11. walking the sign grid's nodes from its root table, as §2.4 does, reaches every leaf, and
    no upper node, lower node, leaf or tile value twice.

A point beyond the `i32` range of coordinates, which rules 8 to 10 reach from the range's
ends, is the background: sign 0, in no leaf.

Rules 8 and 10 hold that only a leaf point owns an edge whose ends differ. Rule 11 refuses
what §2.5 leaves unchecked, nodes shared between entries, so that checking a set costs what
its bytes hold (ADR-0038).

## 5. The narrow-band SDF convention, version 1

A grid of `f32` is an SDF written to this convention when its free pairs hold
`sdf.version` = `1` (ADR-0039):

- voxel v is sampled at the image of v + ½ under the transform, its centre;
- its value is the signed distance there to the surface, in the grid's unit, negative
  inside;
- leaves lie within a band of N voxels of the surface, their values clamped to ±N·h, h
  the voxel size;
- inside tiles hold −N·h; the background is +N·h, which gives N·h to a reader.

A Hermite set on the same tree places its points at the voxels' corners, the SDF's
samples shifted by −h/2.

## 6. Versions

Each format carries its own version: the frozen layout in its header, the cache profile in
its header, Hermite data and the SDF convention in free pairs. A release that raises a
version still reads the one before it and can write it on request; a version older than
that is refused, naming it (ADR-0038). Every format is at version 1, the first.

## 7. Test vectors

The golden files in `crates/brodo/tests/data/`, written by brodo's Format-tier tests
(TESTING.md T1), each fixing its format's bytes:

| File | Holds |
| :--- | :--- |
| `frozen_v1_f32.bin` | a bare frozen buffer of `f32`: leaves, and a tile at each level |
| `frozen_v1_f64x3.bin` | a bare frozen buffer of three `f64`, the same tree |
| `cache_v1.bin` | a cache file of two grids, `f32` and three `f32`, with bounds and a free pair |
| `hermite_v1.bin` | a cache file holding one Hermite set, `golden`, on one leaf, with two named groups |
