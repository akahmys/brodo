# Spike: vdbclass (ROADMAP.md P9, D5)

Measured on 2026-11-06 on an Apple M5 with rustc 1.98.1, against brodo at `64f594a` and
OpenVDB 14.0.0 as Homebrew installs it. Nothing here is a timing, so the load was not held
at rest. Reproduce from the repository's root:

```bash
spikes/vdbclass/run.sh out/spike/vdbclass
```

## Question

brodo writes a grid stating the SDF convention (docs/formats.md §5) to a `.vdb` with
`class` = `level set` (§9.3), but reads no grid back as stating it (ADR-0047), so that an
SDF passed through a `.vdb` loses `brodo.sdf.version`. Option B would state it on reading
for a grid of `class` `level set` that meets four conditions, and keep `class` otherwise:

1. its values are `f32`;
2. its voxel sizes along x, y and z are equal, exactly or within a tolerance;
3. its background is positive and finite;
4. every leaf value lies within ±background, none NaN, and every tile kept is ±background.

Do the level sets OpenVDB's own tools make meet them, and how far apart do the three voxel
sizes brodo reads lie under a rotated map, from OpenVDB's files and from brodo's?

## What was read

Nothing of OpenVDB's source. `levelsets.cpp` calls OpenVDB 14.0.0's public API;
`provenance_guard.py` blocks the `#include` line of the library's main header, as a path
naming OpenVDB, so the owner added that line by hand. One compiler error, in the owner's
terminal, showed one line of the installed `math/Mat4.h`: `Mat4::postRotate` takes an
axis of the enum `Axis` and an angle, which is how the rotations are built.

## The harness

- `levelsets.cpp FILE` writes, with the file's default compression, fifteen grids made by
  OpenVDB's tools, and prints each as OpenVDB reads it: its class, value type, background,
  voxel size, and its leaves' least and greatest value, every voxel active or not.
  - `createLevelSetSphere`: a sphere off the origin; one of radius 100 voxels; one of half
    width 2.5; one of `double`.
  - `createLevelSetCube`, `createLevelSetIcosahedron`.
  - `csg`: two spheres' `csgUnion`, then a third's `csgDifference`.
  - `filter_mean_curvature`: a cube filtered five times by `LevelSetFilter::meanCurvature`.
  - `filter_gaussian_offset`: a cube filtered by `gaussian(2)`, then `offset(0.25)`.
  - `rebuild`: a sphere's `levelSetRebuild` at isovalue 0.1.
  - `meshToVolume` of a UV sphere of 101 760 triangles, slivers at its poles, at voxel
    0.05: under a uniform scale; and of 6240 triangles, under a scale and translation, two
    rotated maps, voxel 0.05 and 1/30, and a non-uniform scale (0.05, 0.05, 0.1).
- `levelsets.cpp rotations FILE N`: N level sets of one voxel, each under a linear map of
  one of five voxel sizes, rotated about x, y and z by pseudo-random angles and translated.
- `src/main.rs FILE` reads a file with brodo's reader and lists each grid's value type,
  `class`, voxel sizes and their spread, (greatest − least) / least in units of
  `f64::EPSILON`, its background, how many leaf values lie outside ±background and by how
  many `f32` steps, how many tiles kept are not ±background, and whether B holds with the
  sizes equal within 16 ε and exactly.
- `src/main.rs brodo N`: N grids of one voxel stating the convention, under maps of the
  same five sizes rotated by pseudo-random unit quaternions and translated, written by
  brodo's writer and read back, listed the same way.
- `spread.py` summarises the spreads of a listing.

## Results

Each of the fifteen grids reads in brodo with the same leaves, and the same least and
greatest leaf value, as OpenVDB lists it.

| Grids | Type | Background | Leaf values outside ±background | Tiles not ±background | Spread | B |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| The four spheres, cube, icosahedron, csg, both filters, rebuild | `f32` | 3 voxels; 2.5 for half width 2.5 | 0 | 0 | 0 | holds |
| `sphere_double` | `f64` | 3 voxels | 0 | 0 | 0 | fails, (1) |
| The four uniform meshes, rotated ones among them | `f32` | 3 voxels | 0 | 0 | 0 | holds |
| `mesh_nonuniform` | `f32` | 3 voxels of 0.05 | 0 | 0 | 0.5 relative | fails, (2) |

Of the leaf values of every grid, none lies outside ±background by a single step: OpenVDB's
tools clamp to the background exactly, filtering and CSG among them.

The spread of the voxel sizes brodo reads under rotated maps:

| Written by | Grids | Equal exactly | Median | 99th centile | Greatest |
| :--- | :--- | :--- | :--- | :--- | :--- |
| OpenVDB 14.0.0 (`rotations`) | 2000 | 901 | 0.5 ε | 1.5 ε | 2.25 ε |
| brodo (`brodo`) | 100 000 | 25 232 | 0.64 ε | 2.25 ε | 4.38 ε |

OpenVDB's own `voxelSize()` of `mesh_rotated` spreads by 7.5 ε where brodo's lengths of the
map's columns agree exactly: the two compute the size differently, so the spread is not a
property of the file alone.

## What it shows

- The four value conditions keep no level set of OpenVDB's tools out but those not `f32`
  or not of equal voxel sizes, and so would turn away only grids that break §5. The check
  passes over values the reader already holds; its cost is not measured here.
- Equality of the voxel sizes cannot be exact: under a rotated map it holds for 45% of
  OpenVDB's grids and 25% of brodo's. A tolerance of 16 ε on the spread keeps every grid
  measured, 3.6 times the greatest spread found (brodo's, 4.38 ε).
- `double` level sets, which OpenVDB's tools make as readily, stay `class` = `level set`
  under B, since §5 holds `f32` alone.
