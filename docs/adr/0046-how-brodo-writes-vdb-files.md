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
