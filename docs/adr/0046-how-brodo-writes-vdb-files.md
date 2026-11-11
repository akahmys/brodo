# ADR-0046: How brodo writes `.vdb` files

- **Status**: Accepted
- **Date**: 2026-10-31

## Context

ADR-0045 had brodo write `.vdb` itself, writing first. The spike `spikes/vdb/` (2026-10-31,
`spikes/vdb/REPORT.md`) wrote brodo's float and double grids from the layout learnt at
OpenVDB `v14.0.0` (PROVENANCE.md): spheres of radius 20 and 80 and an ellipsoid off the
origin, uncompressed and with active-mask compression. OpenVDB 14.0.0 read every file back
with every active voxel and tile the same, compared as doubles, and wrote it again byte for
byte but for its random UUID and its in-memory size, `file_mem_bytes`. A leaf left in
brodo's order was caught, 33 902 values differing on the ellipsoid. Active-mask
compression made a band-only float sphere of radius 20 418 391 bytes against 1 597 998
uncompressed; OpenVDB's own zip and Blosc made its sphere about half that again, each
needing a codec brodo lacks.

## Decision

The owner decided, on 2026-10-31:

- **Compression**: active-mask alone, flags 2, which needs no dependency; zip and Blosc
  wait until a user asks (ADR-0036).
- **Transform**: an `AffineMap` whose translation adds half a voxel, so that OpenVDB's
  index v stands where brodo samples v + ½ (docs/formats.md §5).
- **Class**: `level set` for a grid that holds the SDF convention (`sdf.version`),
  `unknown` for any other.
- **Activity**: every leaf voxel and every tile is written active, since brodo holds no
  per-voxel state (ADR-0025).
- **UUID**: thirty-six characters of zeros and hyphens, so that the same grid gives the
  same bytes (AGENTS.md principle 2); OpenVDB does not check it.
- **Versions**: file version 225 and library 14.0, as OpenVDB 14.0.0 writes them.
- **Types**: `f32` and `f64` grids only.

The bytes are specified in docs/formats.md §9.

## Consequences

A file brodo writes opens in OpenVDB 14 with brodo's values where brodo samples them, and
is larger than OpenVDB's own by the codec it lacks and by the voxels outside the band that
brodo holds active. Whether OpenVDB 12 and 13 read file version 225 was not tried; it is
tried before the reading step settles which files brodo reads. OpenVDB's tools are the
check, by hand and in the spike, not in the gate.

## Amendment: vector grids written

- **Date**: 2026-11-10

Issue 3 asked for `.vdb` grids of `[f32; 3]` and `[f64; 3]`, and of `u8` and `u32`. The
spike `spikes/vdbvec/` (2026-11-10, `spikes/vdbvec/REPORT.md`) had OpenVDB 12.0.0, 13.0.0
and 14.0.0 write vector grids at every compression, full and saved as half floats, and
read every file the others wrote. A vector grid was laid out as a scalar grid, each value
three components x, y, z, of type `Tree_vec3s_5_4_3` or `Tree_vec3d_5_4_3`; a vector type
other than invariant was stated by a `string` entry `vector_type`. None of the three
registered a grid type of `uint8_t` or `uint32_t`, so none of them reads such a grid; the
nearest that were registered, `int32` and `int64`, hold every `u8` and `u32`.

The owner decided, on 2026-11-10, as proposed: brodo writes grids of `[f32; 3]` and
`[f64; 3]` as `Tree_vec3s_5_4_3` and `Tree_vec3d_5_4_3`, by docs/formats.md §9 with a
value of three components, stating no vector type and of `class` `unknown` whatever its
pairs, the SDF convention being a scalar's (§5). `u8` and `u32` are not written to `.vdb`:
no OpenVDB reads them, and widened to `int32` and `int64` they would read back as another
type. OpenVDB 12.0.0, 13.0.0 and 14.0.0 read brodo's vector test vector,
`vdb_vec_v225.bin`, to the values brodo wrote, checked by hand in the spike's oracle, not
in the gate.
