# Spike: thin (B-020, finding)

Measured on 2026-10-26 with rustc 1.98.1 (release), on an Apple M5 (10 cores, 32 GB), at a
one-minute load average near 1.8; times are wall-clock on all ten cores. Reproduce from the
repository's root:

```bash
cargo build --release --manifest-path spikes/thin/Cargo.toml
spikes/thin/target/release/thin-spike > out/thin/results.tsv
ONLY=wedge SHOW=A spikes/thin/target/release/thin-spike > /dev/null   # where a method errs
```

## Question

**B-020**, its first half: how does colino find, from the mesh before converting it, the
places thinner, or gaps narrower, than two voxels (ARCHITECTURE.md §5.2, layer 3)? The
second half, checking them by a local reconversion at h/2, is not measured here.

## Candidates

Points are sampled on every triangle, about h/2 apart (the centroids of a k-fold split).
At each point, with the triangle's normal n taken both ways, so that orientation does not
matter; a side is a wall where its measure's midpoint, or ball's centre, is inside by the
parity count (ADR-0027), otherwise a gap; a place is thin where the measure is below 2h:

| | Measure |
| :--- | :--- |
| A, rays | the distance to the nearest hit along +n and along −n, the point's own triangle left out: a single ray, the shape diameter function of Shapira et al. without its cone, as Cabiddu and Attene (2017, §2.2) describe it |
| B, shrinking ball | on each side, the diameter of the largest ball tangent at the point along ±n holding no point of the mesh: from a radius of 4h, while the mesh's nearest point y to the centre is nearer than the radius, the radius becomes that of the ball tangent at the point through y, \|y − x\|² / 2 n·(y − x). The idea is Ma, Bae and Choi's (The Visual Computer 28(1), 2012), from their abstract only; their paper's text was not to be had openly |
| B90 | B, counting a ball only where the angle at its centre between the point and y exceeds 90°, meant to pass over convex edges |

Cabiddu and Attene's ε-shapes (Computers & Graphics 66, 2017), exact, were not tried: they
need a closed 2-manifold and a constrained tetrahedrization of its hull, a dependency, and
cost O(n²) in the worst case.

## Cases

h = 1. Each case judges some of its points, away from the borders between what is thin and
what is not, as holding a thin wall or a narrow gap, or neither.

| Case | Shape | Judged |
| :--- | :--- | :--- |
| slab_t | a plate 20 × 20 × t, t = 1, 1.5, 2.5, 3, turned about two axes; `_turned`: every third triangle turned over | its two faces, 2 from their edges: thin when t < 2 |
| gap_g | two boxes 10 thick, g apart, g = 0.5, 1.5, 2.5, 3, turned; and `_turned` | the facing faces, narrow when g < 2; the far faces, neither |
| dumbbell | lobes of radius 5 on a neck of radius 0.7, 48 sides | the neck: thin; the lobes' far halves: neither |
| cone | half-angle atan ½, apex at 0, base at 10 | its side below 1: thin; from 6 to 9: neither |
| wedge | a plate 20 long, 0.5 to 4.5 thick, its top slanting | its faces, thin where the thickness across is below 1.6, neither above 2.4 |
| ball_over_plate | a ball of radius 6 0.3 above a plate 4 thick | the plate and the ball's underside: narrow where the gap below the ball is under 2 |
| sphere | radius 8, 64 sides | everywhere: neither |
| torus_thin | tube of radius 0.7, 96 × 12 | everywhere: thin |

## Results

Points found thin of those judged thin, and points found thin that were judged neither
(`out/thin/results.tsv`):

| Case | Judged | Thin | A found / false | B found / false | B90 found / false |
| :--- | -: | -: | -: | -: | -: |
| slab_1, _1.5, and turned | 4140 | 4140 | 4140 / 0 | 4140 / 0 | 4140 / 0 |
| slab_2.5, _3, and turned | 4140 | 0 | 0 / 0 | 0 / 0 | 0 / 0 |
| gap_0.5, _1.5, and turned | 1224 | 612 | 612 / 0 | 612 / 0 | 612 / 0 |
| gap_2.5, _3, and turned | 1224 | 0 | 0 / 0 | 0 / 0 | 0 / 0 |
| dumbbell | 6336 | 2304 | 2304 / 0 | 2304 / 288 | 0 / 288 |
| cone | 2160 | 432 | 432 / 0 | 432 / 288 | 48 / 288 |
| wedge | 2416 | 856 | 856 / 32 | 856 / 189 | 824 / 189 |
| ball_over_plate | 4792 | 2760 | 2760 / 0 | 2760 / 0 | 2760 / 0 |
| sphere | 16512 | 0 | 0 / 0 | 0 / 0 | 0 / 0 |
| torus_thin | 6912 | 6912 | 6912 / 0 | 6912 / 0 | 0 / 0 |

The median measure on the plates and gaps was the thickness or width they were built with,
to three decimals, by every method. On the torus A measured a median of 1.352 for a tube
1.4 across, its 12-sided section's width between faces; B measured 0.451.

Time on the sphere's 16 512 points: A 69 ms, B 36 ms, B90 32 ms; on the ball over the plate,
4792 points: A 43 ms, B 29 ms.

## Findings

1. **A found every thin place judged, on every case, turned triangles included, and called
   thick nothing judged thick but 32 points of the wedge.** Those lie within 0.14 of its
   thick end, where the top meets the end face at 79°: the inward ray leaves through the end
   face 0.7 away. Near an edge sharper than a right angle the solid is that thin; the case
   judged only the plate's thickness.
2. **B is spoiled by the mesh's facets.** A ball tangent at a facet near a convex edge
   between facets is stopped by the next facet, as at a box's edge, so on curved meshes it
   reports curvature at the facets' scale: 0.451 for the torus's tube of 1.4, and 288
   points of the dumbbell's lobes and of the cone called thin. B90's angle passed over
   those, but also over the thin places themselves, every one on the torus and the
   dumbbell's neck.
3. A costs about 4 µs a point on ten cores here, rays crossing the whole mesh's box. At
   points h/2 apart, the sphere of radius 400 (about 8 million points) would take some
   30 s, as long as its conversion (ROADMAP.md §1).
4. Neither A nor B is exact: both sample, and a ray along the normal can miss a thin place
   whose two sides are not opposite along it. None was seen in these cases.

## What this leaves to decide (for the ADR)

- The measure: A, rays along both normals, the side by parity.
- How flagged points become the report's places, "coordinates and extents" (ARCHITECTURE.md
  §5.2): for instance, triangles with a flagged point joined where they share an edge, each
  group a place with its box, its kind and its least measure.
- The sample spacing, h/2 here, and the threshold, 2h.
- The second half of B-020: reconverting each place at h/2 and comparing invariants, which
  needs its own measurement.
