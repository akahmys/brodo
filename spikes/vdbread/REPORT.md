# Spike: vdbread (P7's reading step, ADR-0045)

Measured on 2026-11-01 on an Apple M5 against three OpenVDB libraries: 14.0.0 as Homebrew
installs it, and 12.0.0 and 13.0.0 built by hand from their tags' archives with Boost
1.92.0, c-blosc 1.21.7, TBB 2023.2 and the system's zlib. Reproduce from the repository's
root, with `$S` for `out/spike/vdbread`:

```bash
mkdir -p $S/tar/openvdb-v12.0.0 $S/src/openvdb-v12.0.0
curl -sSfL -o $S/tar/openvdb-v12.0.0/source.tar.gz https://codeload.github.com/AcademySoftwareFoundation/openvdb/tar.gz/v12.0.0
tar -xzf $S/tar/openvdb-v12.0.0/source.tar.gz -C $S/src/openvdb-v12.0.0 --strip-components 1
cmake -S $S/src/openvdb-v12.0.0 -B $S/build/openvdb-v12.0.0 -DCMAKE_BUILD_TYPE=Release -DOPENVDB_BUILD_BINARIES=OFF -DOPENVDB_BUILD_UNITTESTS=OFF -DUSE_EXPLICIT_INSTANTIATION=OFF -DCMAKE_CXX_FLAGS=-Wno-missing-template-arg-list-after-template-kw -DCMAKE_INSTALL_PREFIX=$PWD/$S/inst/openvdb-v12.0.0 -DCMAKE_PREFIX_PATH="$(brew --prefix);$(brew --prefix tbb)"
cmake --build $S/build/openvdb-v12.0.0 -j4 && cmake --install $S/build/openvdb-v12.0.0
# 13.0.0 the same; then the oracle against each library, as oracle12, oracle13, oracle14:
clang++ -std=c++17 -O2 -Wno-missing-template-arg-list-after-template-kw -I $S/inst/openvdb-v12.0.0/include -I$(brew --prefix)/include -I$(brew --prefix tbb)/include spikes/vdbread/oracle/oracle.cpp -L $S/inst/openvdb-v12.0.0/lib -L$(brew --prefix)/lib -L$(brew --prefix tbb)/lib -Wl,-rpath,$PWD/$S/inst/openvdb-v12.0.0/lib -lopenvdb -ltbb -o $S/oracle12
$S/oracle12 info
$S/oracle12 default $S/d12.vdb            # also: zip, none
$S/oracle14 dump $S/d12.vdb > $S/d12.by14.dump
python3 -I spikes/vdbread/decode.py $S/d12.vdb > $S/d12.decoded
python3 -I spikes/vdbread/decode.py --check $S/d12.decoded $S/d12.by14.dump
python3 -I spikes/vdbread/inspect.py $S/d12.vdb
```

## Question

Which files do OpenVDB 12, 13 and 14 write by default for float and double grids, what
must a reader in brodo decode to read them and the files brodo writes, and do OpenVDB 12
and 13 read file version 225 (ADR-0046)?

## Where the format came from

- The archives of the tags `v12.0.0` (commit `269300808a4651daa65836214fc20f78e1c0eb7f`,
  the one ADR-0044 recorded) and `v13.0.0` (`7c03e1f084873cd1b3422c7ff7aec6ee681b3b38`,
  `git get-tar-commit-id`) were built, not read. Their `LICENSE` is the same bytes as at
  `v14.0.0` (SHA-256 `a6cba85b…bf3ff9`), and so is `RE-LICENSE_NOTE.txt` between 12.0.0
  and 13.0.0; 12.0.0 holds no `NOTICE`. One line of `v12.0.0`'s `openvdb/version.h`, that
  its file version is 224, was shown by the compiler in an error; of `v13.0.0`,
  `openvdb/openvdb/io/Archive.cc`'s `Archive::setGridCompression` was read, to learn why a
  file asked for zip held none on some grids. Nothing else of either tag's source was
  opened.
- docs/formats.md §9 and OpenVDB `v14.0.0`'s `io/Compression.h`, read for the writing step
  (PROVENANCE.md), for the layout; c-blosc's public `README_CHUNK_FORMAT.rst` for a Blosc 1
  chunk; the LZ4 project's `lz4_Block_format.md` for LZ4's block.
- Everything else, below, was taken from OpenVDB's output and held to its dump.

## The harness

- `oracle/oracle.cpp`, through OpenVDB's public API alone: `info` gives the library's
  version, the file version it writes and a new file's compression; `default` writes five
  grids with that compression, or with zip or none; `dump` lists every grid's class, map,
  background, each leaf's 512 values with their active bits and each tile with its value
  and active bit. The five grids: a float and a double level set of an ellipsoid off the
  origin, flood-filled as OpenVDB's level sets are, under OpenVDB's default transform; a
  double fog of 160³ voxels of active tiles beside forty distinct voxels, under a uniform
  scale and translation; four float leaves whose inactive voxels hold one value other than
  the background, the background and one other, two others, and three others, under a
  non-uniform scale; and a double upper node of 32 768 active tiles, the largest buffer of
  values a node of the 5-4-3 tree holds.
- `decode.py`, a reader of the whole file in Python, prints what `dump` prints; `--check`
  holds the two listings to each other line for line, every value as `%.17g`.
- `inspect.py` lists the file version, each grid's compression flags and metadata, and over
  its leaf buffers each indicator and codec.

## Measurements

| Library | File version written | Default compression | `default`, bytes | zip, bytes | none, bytes |
| :--- | ---: | :--- | ---: | ---: | ---: |
| 12.0.0 | 224 | Blosc and active-mask (6) | 580 459 | 684 130 | 6 553 073 |
| 13.0.0 | 225 | the same | 580 459 | 684 130 | 6 553 073 |
| 14.0.0 | 225 | the same | 578 476 | 682 146 | 6 552 490 |

- **Reading across versions**: each of the three libraries read each of the nine files and
  brodo's golden file `crates/brodo/tests/data/vdb_v225.bin`, and gave the same dump as
  14.0.0 for every one. 12.0.0 warned, on every file of version 225, `unsupported VDB file
  format (expected version 224 or earlier, got version 225)`, and read it the same. The
  golden file with its version byte set to 224 was read by all three without a warning and
  gave the same dump.
- **The document's reader**: `decode.py` gave, for every one of the nine files and the
  golden file, the same listing as OpenVDB 14.0.0's dump, line for line (810 994 lines a
  file; 229 380 for the golden file). It first differed on 24 487 lines of the level sets,
  with indicator 3's selection bit taken the other way, and on one leaf, with indicator 4's;
  each was fixed to OpenVDB's reading.
- **Compression flags**: Blosc files hold flags 6 on every grid, none 0. Asked for zip,
  OpenVDB wrote flags 3 on the two grids of class `unknown` and 2, active-mask alone, on
  the level sets and the fog: it drops zip for those two classes (`io/Archive.cc`,
  `Archive::setGridCompression`, at `v13.0.0`), but not Blosc. Every node's values,
  internal nodes' included, carry the per-node indicator byte, at flags 0 too, where it is
  6.
- **Indicators**: the level sets' leaves hold 0, 1 and 3; the fog's 0; the four leaves 2,
  4, 5 and 6, all seven so. Read as OpenVDB reads them, an inactive value is: 0, the
  background; 1, minus the background; 2, the one value stored; 3, the background where the
  selection mask's bit is set, else minus the background; 4, the background where it is set,
  else the one value stored; 5, the second value stored where it is set, else the first; 6,
  every value stored.
- **Blosc**: every chunk was Blosc format version 2, LZ4 with byte shuffle, or stored
  (memcpy), its typesize 4 for double grids too, and one block, the 32 768-double buffer
  (262 144 bytes) included; 202 chunks were compressed, split into typesize streams or not
  (bit 4), and 216 stored. A chunk of more than one block, another codec and bit shuffle
  were never written.
- **Maps**: OpenVDB's default transform is `UniformScaleMap`; its fields, and
  `ScaleMap`'s, are fifteen `f64`: scale, voxel size, inverse scale, its square, and half
  the inverse, three each. `UniformScaleTranslateMap`'s are eighteen: the translation,
  then the same fifteen. `AffineMap` is §9.3's. `TranslationMap`, `ScaleTranslateMap`,
  `UnitaryMap` and the frustum map were not written, and are not known from bytes.
- **Activity** in the default file's float level set: 28 982 active leaf voxels, 45 475
  inactive holding the background and 14 631 inactive holding another value, minus the
  background inside; 294 720 inactive tiles holding the background and 10 holding another.
  The fog: 3 905 active tiles.
- **Metadata**: OpenVDB 12.0.0 and 13.0.0 add to every grid `file_delayed_load`, of type
  `__delayedload`, a blob of up to about 1 500 bytes; 14.0.0 as Homebrew built it does not,
  which is the size difference between the files. A reader skips an entry it does not use
  by its byte count.
- Not measured: time; reading half-float (`is_saved_as_half_float`), instanced grids, grid
  types other than float and double, and file versions below 224.

## Findings

1. OpenVDB 12, 13 and 14, built with Blosc, write Blosc with active-mask by default, at
   file version 224 (12) or 225 (13, 14), and the two versions lay out float and double
   grids the same: one reader read all nine files as OpenVDB does.
2. OpenVDB 12.0.0 reads brodo's files of version 225 with every value the same, warning
   that the version is newer than its own; stamped 224, the same bytes read in 12, 13 and
   14 without a warning.
3. Reading OpenVDB's own files needs, beyond what brodo writes: the seven indicators,
   inactive values and a selection mask; Blosc's chunk header and LZ4's block, which
   `lz4_flex` already decodes (`archive`'s dependency, ADR-0016) — only one block was ever
   written, so a reader can refuse more; zip, which OpenVDB never writes by default nor on
   a level set or fog, needs a zlib decoder, a dependency brodo lacks;
   maps other than `AffineMap`; and metadata entries of types it does not know, skipped.
4. OpenVDB's files hold voxels and tiles inactive, most of them holding the background, and
   in a level set some holding minus the background; brodo holds no activity (ADR-0025), so
   a reader must decide what an inactive voxel and an inactive tile become.
