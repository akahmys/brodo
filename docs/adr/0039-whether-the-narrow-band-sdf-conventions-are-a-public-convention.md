# ADR-0039: Whether the narrow-band SDF's conventions are a public convention

- **Status**: Accepted
- **Date**: 2026-10-25

## Context

colino's SDF follows choices made for it (ADR-0027): voxel v is sampled at its centre, the
image of v + ½ under the grid's transform; values are signed distances, negative inside,
in the grid's unit; leaves lie in a band of N voxels, values clamped to ±N·h; inside tiles
hold −N·h and the background +N·h. Two things reached past colino:

- Hermite data, already a brodo format any producer writes (ADR-0032), places its lattice
  at the corners of the SDF's voxels, the SDF's sampling lattice shifted by −h/2.
- A producer other than colino, one building SDFs from analytic shapes, writes its SDFs to
  the same conventions (the owner, 2026-10-24).

## Decision

The owner chose, on 2026-10-25, the recommended option, (1):

- (1) **A public convention.** brodo states the conventions in the format document
  (ADR-0037): sample position, sign, unit, band and clamp, tiles and background, with a key
  in `GridMeta::extra` naming the convention and its version, so that a reader can tell an
  SDF written to it; colino writes that key. A producer outside colino can then claim it.
- (2) **colino's choices only**, as now, other producers matching them by reading ADR-0027.

*Recommended: (1)*: the Hermite format already depends on the sampling lattice, and two
producers already write to the same conventions.

## Consequences

Under (1), brodo gains no SDF operation (ARCHITECTURE.md §1 keeps level-set operations out
of scope): it states a convention, and at most checks a grid's key and values against it.
