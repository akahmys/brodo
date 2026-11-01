# Spike: vdb (P7's writing step, ADR-0045)

Measured on 2026-10-31 with rustc 1.98.1, release build, on an Apple M5, against
OpenVDB 14.0.0 as Homebrew installs it (`libopenvdb.14.0.0.dylib`, `vdb_print`).
Reproduce from the repository's root:

```bash
cargo build --release --manifest-path spikes/vdb/Cargo.toml
P=$(brew --prefix); T=$(brew --prefix tbb); clang++ -std=c++17 -O2 -I$P/include -I$T/include spikes/vdb/oracle/oracle.cpp -L$P/lib -L$T/lib -lopenvdb -ltbb -o out/spike/vdb/oracle
spikes/vdb/target/release/vdb-spike write out/spike/vdb/b.vdb ellipsoid20 mask double
out/spike/vdb/oracle dump out/spike/vdb/b.vdb > out/spike/vdb/b.dump
python3 -I spikes/vdb/compare.py out/spike/vdb/b.vdb.tsv out/spike/vdb/b.dump
out/spike/vdb/oracle rewrite out/spike/vdb/b.vdb out/spike/vdb/b.r.vdb mask
```

## Question

Which bytes of a `.vdb` file does OpenVDB 14.0.0 read back as brodo's float or double grid,
and what does a writer of them need from brodo?

## Where the format came from

OpenVDB at `v14.0.0` (`ecae2bda85744412b0cd47e1b8e1e4e76beb7b88`), 26 files, each stating
`SPDX-License-Identifier: Apache-2.0`, read for the layout a writer emits; the files and
the parts read are in PROVENANCE.md (ADR-0044). No code was copied. Facts the files read
left open, each read from the bytes of files OpenVDB wrote (ADR-0045): a string is a `u32`
byte count and its bytes, no terminator; metadata entries come sorted by name, of type
names `string`, `vec3i`, `int64`; a `Vec3d` is three `f64`.

## The harness

- `src/main.rs` builds brodo's grid as the benchmark does (a leaf where the band passes, an
  inside lower tile below it), turns it into OpenVDB's nodes (root, 32³, 16³, leaves, each
  place's bit with z fastest, a leaf's values reordered from brodo's x fastest), and writes
  the file: header, one grid, its metadata, an `AffineMap`, topology and leaf buffers, with
  compression flags 0 (`none`) or active-mask alone (`mask`). Beside it, it lists brodo's
  active boxes and values from the topology and field themselves, not from the nodes
  written.
- `oracle/oracle.cpp`, through OpenVDB's public API alone: `dump` reads a file and lists
  every active voxel and tile with its value; `rewrite` reads a file and writes it again;
  `sphere` writes a reference file.
- `compare.py` holds the two listings to each other: background, the set of active boxes,
  and each value exactly, as a double.

Three cases: spheres of radius 20 and 80 about the origin, and an ellipsoid of radius 20
centred at (3, −5, 7) with axes scaled 1, 1.3 and 0.8. Each case refuses to be written if
it holds no leaf (PLANNING.md §3). The double grid holds each float value plus a part below
`f32`'s precision, so that a value written through `f32` shows.

## Measurements

| Type | Case | Leaves, tiles | Boxes | `none` | `mask` | Values differing | Rewritten by OpenVDB, bytes differing |
| :--- | :--- | ---: | ---: | ---: | ---: | ---: | :--- |
| float | sphere 20 | 158, 8 | 80 904 | 1 597 998 | 418 391 | 0 | UUID and `file_mem_bytes` only |
| float | ellipsoid 20 | 174, 10 | 89 098 | 1 632 830 | 453 231 | 0 | the same |
| float | sphere 80 | 2 606, 3 025 | 1 337 297 | 6 927 294 | 5 759 755 | 0 | the same |
| double | sphere 20 | the same | 80 904 | 3 101 235 | 742 012 | 0 | the same |
| double | ellipsoid 20 | the same | 89 098 | 3 168 835 | 809 628 | 0 | the same |
| double | sphere 80 | the same | 1 337 297 | 13 444 035 | 11 108 948 | 0 | the same |

- `vdb_print -l` read every file as version 14.0/225, `Tree_float_5_4_3` or
  `Tree_double_5_4_3`, its active voxels and tiles as brodo holds them.
- OpenVDB's `rewrite` of each file, at the same compression, was the same length, and
  differed only in the 36-byte UUID, random in OpenVDB's, and in `file_mem_bytes`, the size
  in memory OpenVDB states for information, which the spike writes as 0.
- Shown to fail: with a leaf's values left in brodo's order (x fastest), the ellipsoid's
  file gave 33 902 values differing; the sphere's did not, since swapping x and z leaves a
  sphere about the origin unchanged, which is why the ellipsoid is a case. The double
  ellipsoid's listing against the float file's dump gave 88 561 values differing.
- For comparison, OpenVDB's own writer, on a float sphere of radius 20 with only the band
  active (not brodo's tree): 1 597 996 bytes uncompressed, 215 789 with zip and active-mask,
  203 322 with Blosc and active-mask, its default.
- Not measured: time. The machine's one-minute load was 3.87 when the writes were run.

## Findings

1. brodo's tree has OpenVDB's default shape, and a writer of a few hundred lines gives
   files OpenVDB 14.0.0 reads back with every value the same, float and double, and writes
   again byte for byte but for its random UUID and its in-memory size.
2. Every leaf voxel and tile is written active, since brodo holds no per-voxel state
   (ADR-0025); OpenVDB's own level sets leave voxels outside the band inactive, so the
   same sphere is 158 leaves of all-active voxels in brodo's file.
3. Active-mask compression needs nothing brodo lacks and cut a band-only grid to a quarter
   (418 391 against 1 597 998 bytes); where leaves dominate, as at radius 80, it cut a
   sixth. Zip needs a zlib encoder, and Blosc a Blosc frame around `lz4_flex`'s block
   format; either cut OpenVDB's own sphere to an eighth.
4. brodo places voxel v's corner at the transform's image of v and samples an SDF at
   v + ½ (docs/formats.md §5); OpenVDB samples at the index itself. The spike's
   `AffineMap` adds half a voxel to the translation, so that each value stands where brodo
   samples it; OpenVDB read the matrix back as written.
5. OpenVDB checks neither the UUID nor the library version a file states; the spike writes
   a UUID of zeros, so that the same grid gives the same bytes (AGENTS.md principle 2), and
   claims library 14.0 with file version 225. Whether OpenVDB 12, whose own version is
   older, reads version 225 was not tried.
