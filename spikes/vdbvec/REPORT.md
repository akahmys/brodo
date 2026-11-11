# Spike: vdbvec (P12's issue 3)

Measured on 2026-11-10 on an Apple M5 against brodo at `153fd00` and three OpenVDB
libraries: 14.0.0 as Homebrew installs it, and 12.0.0 and 13.0.0 built by hand as
`spikes/vdbread/REPORT.md` builds them. Nothing here is a timing. Reproduce from the
repository's root, once `spikes/vdbread/`'s libraries are built:

```bash
spikes/vdbvec/run.sh out/spike/vdbvec
```

## Question

Issue 3 asks for `.vdb` grids of `[f32; 3]` and `[f64; 3]`, and of `u8` and `u32`. What do
OpenVDB 12, 13 and 14 write for vector grids, at each compression and saved as half
floats, what must brodo's reader (docs/formats.md §9.7, §9.8) take beyond a scalar grid's,
and does OpenVDB have a grid type of `u8` or `u32` at all?

## What was read

Nothing of OpenVDB's source or headers. `oracle/oracle.cpp` calls each library's public
API; `scripts/hooks/provenance_guard.py` blocks the `#include` lines of the library's main
header and its `io/File.h`, as paths naming OpenVDB, so the owner added those two lines by
hand. The layout below was taken from the libraries' output and held to their own reading
of it, as `spikes/vdbread/` did for scalar grids.

## The harness

- `oracle/oracle.cpp`, built against each library as `oracle-12`, `oracle-13`, `oracle-14`:
  - `info`: the library's version, the file version it writes, its default compression.
  - `types`: whether OpenVDB has registered a grid type of each of thirteen tree type
    names, and the tree type name of a tree of `uint8_t`, `uint32_t`, `Vec3s` and `Vec3d`.
  - `write FILE MODE`: three grids. `v3s`, of `Vec3s`, its vector type covariant, under a
    uniform scale of 0.5; `v3d`, of `Vec3d`, its vector type not set, under 0.25; and
    `v3s_empty`. Each of the two has seven leaves whose inactive voxels hold, one leaf
    each, the seven patterns by which OpenVDB writes a node's inactive values
    (docs/formats.md §9.8, indicators 0 to 6), with every 41st voxel active, and an active
    tile at each level and an inactive lower tile. A value's three components are
    distinct, so that one swapped or dropped shows. MODE is the file's compression,
    `default` (Blosc and active-mask), `zip`, `none` or `mask`, and with `half-` before
    it, or `half` alone for the default, `v3s` is saved as half floats.
  - `dump FILE`: each grid as the library reads it: its name, type, class, vector type,
    half flag, active voxel count, metadata, map, background, then every leaf's 512
    values and active bits and every tile, sorted, components joined by commas.
- `decode.py`, `spikes/vdbread/decode.py` taken to values of three components, reads a
  file as brodo would and prints it as `dump` does. `--full BG INACTIVE` tries a half
  grid's background and root tiles, and its inactive values, stored as `f` (full) or `e`
  (half); `--empty` tries a half grid's buffer of no values written without its size.
  `--check` compares two listings line by line.
- `run.sh` builds the three oracles, writes the eight modes with each, dumps and decodes
  each file and checks the two listings, then reads every file with every oracle.

## Measurements

`info`: 12.0.0 writes file version 224; 13.0.0 and 14.0.0 write 225; each defaults to
Blosc with active-mask (flags 6).

`types`, the same in all three: `Tree_vec3s_5_4_3` and `Tree_vec3d_5_4_3` are registered,
as are `bool`, `float`, `double`, `int32`, `int64`, `vec3i` and `mask`. A tree of
`uint8_t` is named `Tree_uint8_5_4_3` and one of `uint32_t` `Tree_uint32_5_4_3`, and
neither is registered, nor `int8` or `uint16`.

Decoding against dumping, 24 files (3 libraries × 8 modes): every listing is the same as
the library's own, 147 484 lines each, once the decoder takes the layout of Findings 1 to 4.

Reading across versions: every library reads every other's 24 files to the same listing
as their writer's, but the file version's line. OpenVDB 12.0.0 reads file version 225
with a warning on stderr that it expects 224 or earlier.

File sizes: 12.0.0's and 13.0.0's default files are 45 800 bytes, 14.0.0's 45 530; the
difference is Finding 5.

Blosc chunks of the default files, by flags, type size, byte count and block size:

| File | Chunks |
| :--- | :--- |
| 14.0.0, default | type size 4 in every chunk, `Vec3s` and `Vec3d` alike; every LZ4 chunk one block (6144 and 12 288 bytes of a whole leaf, 156 and 312 of a leaf's active values); chunks of 0, 12 and 24 bytes stored as they are (flag bit 1) |
| 12.0.0, default | the same |
| 14.0.0, half | `v3s`'s chunks of 6 and 78 bytes stored as they are, of block size 4 and 76, so of two blocks; its LZ4 chunk of 3072 bytes one block |

## Findings

1. **A vector grid is a scalar grid with a value of three components.** Its type is
   `Tree_vec3s_5_4_3` or `Tree_vec3d_5_4_3`; its background, tile values, inactive values
   and stored values are each three `f32` or `f64` in the order x, y, z; the header,
   descriptor, metadata, map, topology, masks, the indicators 0 to 6 and their meanings
   (minus the background is minus each component), and Blosc's chunks are as for a
   scalar grid (docs/formats.md §9). brodo's Blosc decoder already takes them: it splits
   and unshuffles by the chunk's own type size, 4 here, not by the value's size.
2. **The vector type is metadata.** A grid whose vector type is set writes a `string`
   entry `vector_type`, here `covariant`; one left invariant writes none. Under §9.7 rule
   4 a reader keeps it among the grid's free pairs.
3. **A half grid is not a vector grid saved small throughout.** Its type has `_HalfFloat`
   appended and its metadata holds `is_saved_as_half_float`, a `bool`, true. Its
   background, its root tiles' values and every node's inactive values (indicators 2, 4
   and 5) are stored in full, as `f32`; only the values stored after them, upper, lower
   and leaf, are `f16`. A buffer of no values, as an upper or lower node with no active
   tile writes under active-mask, is written with zip or Blosc as nothing at all, without
   the `i64` size a full grid writes before it; without compression there is no size
   either way. Blosc's chunks of a half grid's few values may have a byte count not a
   multiple of the type size and so two blocks, but those met were stored as they are.
4. **Every compression reads alike.** Default, zip, none and active-mask, full or half,
   decode to the libraries' listings by the same rules; zip is zlib, which brodo's reader
   refuses today (§9.7 rule 3).
5. **OpenVDB 12 and 13 write a metadata entry 14 does not.** Each grid's metadata has a
   `file_delayed_load` entry of type `__delayedload`, which their own reading keeps to
   itself; 14.0.0 writes none. Its name begins `file_`, so §9.7 rule 4 already drops it
   and skips its bytes by its count.
6. **OpenVDB has no grid of `u8` or `u32`.** Neither 12, 13 nor 14 registers a grid type
   for either, so none of them can read such a grid: a `.vdb` of `u8` or `u32` would be
   read by brodo alone. Their nearest registered types are `int32`, holding every `u8`,
   and `int64`, holding every `u32`.

## What this leaves to decide

- Whether brodo writes and reads `[f32; 3]` and `[f64; 3]` as `Tree_vec3s_5_4_3` and
  `Tree_vec3d_5_4_3` by Finding 1, its writer stating no vector type (Finding 2).
- Whether a half grid, scalar or vector, stays skipped by name (§9.7 rule 2) or is read
  by Finding 3.
- For `u8` and `u32`, by Finding 6: no `.vdb` at all, or one widened to `int32` and
  `int64`, which OpenVDB reads but brodo then reads back as another kind.
