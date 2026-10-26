# ADR-0028: The topology report counts Betti numbers and region connectivity

- **Status**: Amended
- **Date**: 2026-10-20

## Context

P2's fourth item is the topology report's first two layers (ARCHITECTURE.md §5.2,
ADR-0017): the Betti numbers of the solid and its complement, checked against the input,
and whether regions a user marks lie in one component of the complement. Labelling, the
Euler characteristic and whether a component reaches the outside exist (ADR-0013,
ADR-0024); tests showed they give a ball, a shell, a torus and a double torus their Betti
numbers. The third layer, places thinner than two voxels, is after P2.

## Decision

The human decided, on 2026-10-20:

- The solid is φ < 0 under 26-connectivity, its complement φ ≥ 0 under 6, the pairing the
  Euler characteristic of closed cubes agrees with. β₀ of the solid counts its
  components, β₂ the complement's components that do not reach the outside, and
  β₁ = β₀ + β₂ − χ.
- The complement's Betti numbers are counted, not derived: its components, the solid's β₁
  and the solid's components less one, as duality in the sphere gives; tests hold the
  counts to the duality, so that a disagreement shows.
- The reference is the input's own V − E + F when, welded, it is a closed 2-manifold:
  every edge shared by exactly two triangles in opposite directions, every vertex's
  triangles one fan, no triangle with a repeated corner. Then the solid's χ is the
  surface's half, and a grid whose χ differs is reported, with the difference. β₀ is
  shown beside, not compared, since nested shells make one solid of two surfaces.
- An input that is not a closed 2-manifold has no reference in P2; the report says why,
  counting open edges, edges of more than two triangles, edges whose triangles disagree
  in direction, fanless vertices and repeated corners. Reconverting risky places at h/2
  comes with the third layer.
- Regions are named points in world coordinates. Each is answered with the complement
  component of the voxel holding it, whether that reaches the outside, or that it lies in
  the solid; a point beyond the tree lies outside. Pairs in one component and pairs apart
  are listed.
- The first layer's numbers are the whole grid's; where a change lies comes with the third
  layer, and the report says so.
- The report is plain data, serialisable with `serde` behind colino's feature `serde`, off
  by default; `serde` is checked against Rules 12, 13 and 20 and approved before it is
  added.

## Consequences

Two units, the input's reference and the report, then `serde`. A grid that closes a gap or
opens a wall shows only as a different χ or a different count until the third layer.

## Amendment

- **Date**: 2026-10-20

Writing the report showed that only the complement's β₀ can be counted on its own. Under
the 26/6 pairing, a lattice vertex adds to the 6-connected complement's χ exactly what it
adds to the 26-connected solid's, for all 256 patterns of its 8 voxels (the tables in
`brodo::euler`, compared pattern by pattern), so a counted χ of the complement is the
solid's χ again, and with it β₁. Its β₂ would be the solid's components, already counted.
The human decided:

- The complement's β₀ is counted, by 6-connected labelling, and tests hold it to 1 + the
  solid's β₂, which fails unless exactly one complement component reaches the outside.
  Its β₁ and β₂ are given by duality in the 3-sphere: β₁ the solid's, β₂ the solid's
  components less one, or 1 when there is no solid.
- `serde` 1.0.229 with `derive` is colino's optional dependency behind `serde`, and
  `serde_json` 1.0.154 a dev-dependency for the test that writes a report and reads it
  back. Checked in a scratch crate on 2026-10-20: every crate MIT OR Apache-2.0, except
  `zmij` (MIT) and `memchr` (Unlicense OR MIT, already present); no `cc` on either target,
  the build scripts of `serde` and `serde_core` running only `rustc --version`;
  `cargo deny check` passing. Besides them, `serde_core`, `serde_derive`, `itoa` and `zmij`
  are new; `proc-macro2`, `quote`, `syn` and `unicode-ident` are already present.

## Amendment

- **Date**: 2026-10-26

B-020 asked how the third layer finds places thinner, or gaps narrower, than two voxels
before conversion. `spikes/thin/` sampled points h/2 apart on every triangle and measured,
on both sides of each, a single ray along the normal (the shape diameter function of
Shapira et al. without its cone, as Cabiddu and Attene, *Epsilon-shapes*, Computers &
Graphics 66, 2017, §2.2, describe it) and the largest tangent ball holding no point of the
mesh (after the abstract of Ma, Bae and Choi, The Visual Computer 28(1), 2012). The rays
found every thin wall and narrow gap the cases judged, on plates, gaps, a neck, a cone's
tip, a slanting plate, a ball over a plate and a thin torus, turned triangles included,
calling thick nothing but points within 0.14 voxels of an edge sharper than a right angle;
the balls were stopped by the mesh's facets and called curved surfaces thin (its
REPORT.md). Cabiddu and Attene's exact ε-shapes need a closed 2-manifold and a
tetrahedrization. The human decided:

- Each point sampled h/2 apart on each triangle measures the distance to the nearest
  other triangle along its normal, both ways. A side whose midpoint is inside, by the
  conversion's sign as Hermite data's points are, is a wall that thick; otherwise a gap
  that wide. Below 2h, the point is thin.
- Thin points within h of each other are one place, reported with its kind, its box in
  world coordinates, its least thickness or width, and its count of points.
- Checking a place by reconverting it at h/2 is measured apart, before it is built, and
  recorded here.
