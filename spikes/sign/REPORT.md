# Spike: sign (B-016)

Measured on 2026-10-26 with rustc 1.98.1 (release), on an Apple M5 (10 cores, 32 GB),
every method on all ten cores. Counts do not depend on load; times were taken with the
spike itself loading the machine (a one-minute load average near 20 after the runs), so
they are rough and only compare methods with each other. Reproduce from the repository's
root:

```bash
cargo build --release --manifest-path spikes/sign/Cargo.toml
DEDUP=0 spikes/sign/target/release/sign-spike > out/sign/results-dedup0.tsv
DEDUP=1 spikes/sign/target/release/sign-spike > out/sign/results-dedup1.tsv
ONLY=cube_all_in SHOW=D1-3 spikes/sign/target/release/sign-spike   # where one method errs
for t in colino dedup offset sos; do TIE=$t spikes/sign/target/release/sign-spike > out/sign/tie-$t.tsv; done
```

`TIE` overrides `DEDUP`; `sos` applies to the three axes only.

## Question

**B-016**: what sign does colino offer, beside the winding number's, that does not depend
on a mesh's orientation (ADR-0017)? Today the sign is the generalised winding number
w ≥ 1/2, so a face turned in moves the inside: ADR-0030 measured a false surface inside a
cube with one face turned in, which Hermite data refuses beyond the band.

## Cases

Each case is a broken mesh and the mesh it was meant to be. The truth at a point is the
intended mesh's exact winding number ≥ 1/2; a dense grid of voxel centres, h = 0.5, covers
its box and 6 voxels beyond. A wrong sign within 3 voxels of the intended mesh is counted
in the band, one farther beyond it: a wrong sign beyond the band is a false surface.

| Case | Broken mesh | Voxels | Band |
| :--- | :--- | -: | -: |
| sphere | a sphere of radius 10, 2048 triangles, as meant (control) | 140 608 | 30 312 |
| cube_face_in | a cube of half-side 6.3 with its +z face turned in (ADR-0030's) | 54 872 | 22 555 |
| cube_all_in | the cube with every face turned in | 54 872 | 22 555 |
| sphere_flip10, _flip50 | 10 % and 50 % of the sphere's triangles turned, chosen at random | 140 608 | 30 312 |
| sphere_hole1 | one triangle removed, about a voxel across | 140 608 | 30 312 |
| sphere_hole3 | a cap removed, a hole three voxels across | 140 608 | 30 312 |
| sphere_double | every triangle twice, exactly | 140 608 | 30 312 |
| boxes_overlap | two boxes passing into each other, each as meant | 24 336 | 10 110 |
| boxes_overlap_face_in | the same, a face of one turned in | 24 336 | 10 110 |
| hollow | the sphere with a cavity of radius 6, both as meant | 140 608 | 41 288 |

Four more cases were added after method E was written from the results of those above,
to test it on cases it was not made from: the sphere doubled by a second shell of radius
10.02 (`sphere_double_offset`), two boxes sharing a face (`boxes_touching`), the hollow
sphere with 10 % turned (`hollow_flip10`), and the one-triangle hole with 10 % turned
(`hole1_flip10`).

## Methods

| | Sign | From |
| :--- | :--- | :--- |
| W | w ≥ 1/2, Barill et al.'s approximation at β = 2: colino's today | ADR-0027 |
| A | \|w\| ≥ 1/2 | ROADMAP.md §3 |
| B | a flood from the grid's boundary along the lattice edges that cross no triangle; what it does not reach is inside | ROADMAP.md §3 |
| C | B's regions, each inside when its mean \|w\| is ≥ 1/2, the region reaching the boundary outside | B and A |
| D1-k | parity count along lines through each voxel in k directions, a majority vote; a line with an odd number of crossings does not vote, and a tie is outside | Nooruddin and Turk (2003), §3.1 |
| D2-k | ray stabbing: inside between a line's first and last crossing, and only where every direction says so | Nooruddin and Turk (2003), §3.2 |
| E | a region of B inside when the fewest walls between it and the boundary is odd, otherwise where \|w\| ≥ 1/2 | written here after the first results |

k is 3, the axes, or 13, the axes and the ten directions of an icosahedron's face normals,
as the paper uses. Crossings come from `colino::intersect::crossing`, which hits both
triangles of an edge a line passes exactly on, by design, so that no segment passes between
them (ADR-0030). With `DEDUP=1`, crossings of one line at the same fraction count once.

## Results

Wrong signs, in the band / beyond it, `DEDUP=1` (`out/sign/results-dedup1.tsv`):

| Case | W | A | B | C | E | D1-3 | D1-13 | D2-3 | D2-13 |
| :--- | -: | -: | -: | -: | -: | -: | -: | -: | -: |
| sphere | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 |
| cube_face_in | 3719 / 2695 | 3719 / 2695 | 0 / 0 | 0 / 0 | 1807 / 1462 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 |
| cube_all_in | 8766 / 6859 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 |
| sphere_flip10 | 472 / 0 | 472 / 0 | 0 / 0 | 0 / 0 | 151 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 |
| sphere_flip50 | 12810 / 20480 | 12763 / 20480 | 0 / 0 | 12832 / 20480 | 38 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 |
| sphere_hole1 | 0 / 0 | 0 / 0 | 12832 / 20480 | 12832 / 20480 | 0 / 0 | 0 / 0 | 0 / 0 | 6 / 34 | 84 / 169 |
| sphere_hole3 | 0 / 0 | 0 / 0 | 12832 / 20480 | 12832 / 20480 | 0 / 0 | 0 / 0 | 0 / 0 | 96 / 544 | 720 / 1736 |
| sphere_double | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 |
| boxes_overlap | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 768 / 32 | 768 / 32 | 0 / 0 | 0 / 0 |
| boxes_overlap_face_in | 676 / 20 | 676 / 20 | 0 / 0 | 0 / 0 | 382 / 10 | 768 / 32 | 768 / 32 | 0 / 0 | 0 / 0 |
| hollow | 0 / 0 | 0 / 0 | 4192 / 3016 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 4192 / 3016 | 4192 / 3016 |
| *held out* | | | | | | | | | |
| sphere_double_offset | 240 / 0 | 240 / 0 | 240 / 0 | 240 / 0 | 240 / 0 | 13072 / 20480 | 13072 / 20480 | 240 / 0 | 240 / 0 |
| boxes_touching | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 |
| hollow_flip10 | 517 / 0 | 517 / 0 | 4192 / 3016 | 0 / 0 | 148 / 0 | 0 / 0 | 0 / 0 | 4192 / 3016 | 4192 / 3016 |
| hole1_flip10 | 410 / 0 | 410 / 0 | 12832 / 20480 | 12832 / 20480 | 410 / 0 | 0 / 0 | 0 / 0 | 6 / 34 | 84 / 169 |

Without `DEDUP` (`results-dedup0.tsv`) only D1 changes: 6 / 19 wrong on both cubes, along
the diagonal x = y = z, where each axis line passes exactly on the edge splitting a face
and is counted twice, so every line is odd and none votes; 12 / 12 on `boxes_touching`;
and `sphere_double` wholly wrong, 12832 / 20480, as `sphere_double_offset` is with it. With
it, `sphere_double`'s exactly repeated triangles count once, which is why D1 passes that
case and not the offset one. The 240 voxels every method but D1 counts wrong in
`sphere_double_offset` lie between the two shells, outside the intended mesh.

Times on the sphere's 140 608 voxels, in ms, `DEDUP=0`, rough: W 46, B 16, C and E 62
(they include W and B), D1-3 10, D2-3 12, D1-13 60, D2-13 60. On the dense grid, three
directions cost less than the winding number; colino samples only the band's leaves and a
point per inside region, where lines cannot be shared as they are here.

## Findings

1. No method is right on every case. Each fails on one kind of defect, wholly:

   | Method | Turned faces | Holes | Cavities | Passing parts | Double walls |
   | :--- | :-: | :-: | :-: | :-: | :-: |
   | W (today) | fails | holds | holds | holds | holds |
   | A | only when all are turned | holds | holds | holds | holds |
   | B | holds | fails: leaks through a hole of one triangle | fails: fills it | holds | holds |
   | C | fails at 50 % | fails | holds | holds | holds |
   | D1 | holds | holds, by the vote | holds | fails in the overlap | fails |
   | D2 | holds | nearly: errors near the hole | fails: fills it | holds | holds |

2. **D1, parity with three directions, is the one orientation-independent method that
   held on turned faces, holes and cavities together**, and on the held-out mixes of them
   (`hollow_flip10`, `hole1_flip10`); thirteen directions changed nothing here. It fails,
   as the paper states, where parts pass into each other or walls are doubled: there W
   holds.
3. D1 needs a crossing counted once where a line passes exactly on a shared edge. colino's
   `crossing` counts it in both triangles by design (ADR-0030); dropping equal fractions
   did it here, but would also count a line grazing a silhouette edge once rather than
   twice or not at all, and merges exactly repeated triangles. A rule of its own is needed
   before D1 is built, such as moving the lines off the lattice by a fixed offset, or a
   half-open rule on the edge functions.
4. D2 is the paper's answer to passing parts and double walls, but fills cavities and errs
   near holes; it holds where D1 fails and fails where W holds.
5. E, written after seeing the first results, did not hold: near a turned face the
   outside's |w| passes 1/2 (`cube_face_in`). It is not proposed.

## What this leaves to decide (for the ADR)

- Which method colino offers as the orientation-independent option: D1 alone; D1 and D2
  for the user to choose by defect, as the paper leaves it; or none.
- D1's rule for a line on an edge, measured before it is built.
- Whether the option is per conversion and Hermite data alike (ROADMAP.md §2), and that W
  stays the default.

## A line on an edge (2026-10-26)

After the human chose parity along the three axes (ADR-0027), the same spike measured how
a line exactly on a shared edge or corner counts once. `TIE` picks the rule:

| Rule | How |
| :--- | :--- |
| `colino` | `colino::intersect::crossing` as it is: both triangles of the edge are hit |
| `dedup` | crossings of one line at the same fraction count once |
| `offset` | each line moved across itself by (0.003141 h, 0.002718 h), as fixed fractions |
| `sos` | the spike's own edge test: an edge function exactly 0 takes the sign it has with the line moved by (ε, ε²) across itself, each product's sign exact through a fused multiply-add; an edge's two triangles then hold the moved line on one side, and a corner's fan in one sector |

Two cases were added for it: an octahedron of radius 6.25 whose vertices lie on the lines
through voxel centres (`octa_lattice`), and the same with every third triangle turned
(`octa_lattice_flip`). Wrong signs of D1-3, in the band / beyond it (`out/sign/tie-*.tsv`):

| Case | colino | dedup | offset | sos |
| :--- | -: | -: | -: | -: |
| cube_face_in, cube_all_in | 6 / 19 | 0 / 0 | 0 / 0 | 0 / 0 |
| octa_lattice, octa_lattice_flip | 570 / 295 | 0 / 0 | 0 / 0 | 0 / 0 |
| boxes_touching | 12 / 12 | 0 / 0 | 0 / 0 | 0 / 0 |
| the seven spheres, holed, turned and hollow among them | 0 / 0 | 0 / 0 | 20 / 0 each | 0 / 0 |
| sphere_double, every triangle twice | 12832 / 20480 | 0 / 0 | 12832 / 20480 | 12832 / 20480 |
| sphere_double_offset, boxes_overlap(_face_in) | as before | as before | as before | as before |

The perturbation counted every line on an edge or corner once and changed no other sign;
its time matched `colino`'s. The offset signs the moved point, not the voxel's centre, and
so missed 20 voxels nearest each sphere. `dedup` passed exactly repeated triangles by
merging them, which no rule for single crossings should do; the perturbation, like parity
itself, takes them for a double wall.
