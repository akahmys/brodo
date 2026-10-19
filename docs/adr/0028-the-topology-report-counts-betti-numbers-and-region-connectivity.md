# ADR-0028: The topology report counts Betti numbers and region connectivity

- **Status**: Accepted
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
