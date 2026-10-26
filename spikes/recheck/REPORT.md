# Spike: recheck (B-020, checking)

Measured on 2026-10-26 with rustc 1.98.1 (release), on an Apple M5 (10 cores, 32 GB).
Invariants do not depend on load; times are rough, on all ten cores. Reproduce from the
repository's root:

```bash
cargo build --release --manifest-path spikes/recheck/Cargo.toml
spikes/recheck/target/release/recheck-spike > out/recheck/results.tsv
```

## Question

**B-020**, its second half: can the topology report check each place its third layer
finds (ADR-0028) by reconverting it at h/2 and comparing invariants?

## Method

Each place's box is grown by 2h and aligned to the h grid's cells, which the h/2 grid
splits, so that both see the same box. In it the solid is taken three ways:

- at h, the conversion's voxels, those with a negative value;
- at h/2, the voxel centres the conversion's sign puts inside (the winding number at
  Barill et al.'s β = 2), without converting the mesh again;
- at h/8, the same, as this spike's reference only.

Of each, three invariants: χ of the union of its closed voxels, the solid's components
under 26-connectivity, and the box's other voxels' under 6, the conversion's pairing
(ADR-0028). A place **changed** where h's invariants differ from the reference's; it is
**flagged** where they differ from h/2's.

## Cases

h = 1 but where named; each at the world's origin and at (0.37, 0.21, 0.43): plates 12 × 12
× t turned about two axes, t from 0.3 to 1.9; two boxes g apart, g from 0.3 to 1.9; the
report's shell, radius 20 round a cavity of 19, at h = 2 and 1.5; a dumbbell of lobes 5 on
necks of radius 0.3 to 0.9.

## Results

Invariants (χ, solid components, other components), `out/recheck/results.tsv`:

| Case | at h | at h/2 | reference | changed | flagged |
| :--- | :--- | :--- | :--- | :-: | :-: |
| plate 0.3 | (22, 22, 1), (21, 21, 1) | (−61, 1, 1) | (1, 1, 1) | yes | yes |
| plate 0.5 | (−10, 2, 1), (−8, 2, 1) | (1, 1, 1) | (1, 1, 1) | yes | yes |
| plate 0.7 | (−14, 1, 1), (−11, 1, 1) | (1, 1, 1) | (1, 1, 1) | yes | yes |
| plate 0.9 to 1.9 | (1, 1, 1) | (1, 1, 1) | (1, 1, 1) | no | no |
| gap 0.3, 0.5 | (−2 or −1, 1, 1) | (−4 or −3, 1, 1) | (2, 2, 1) | yes | yes |
| gap 0.7, 0.9 | (−1, 1, 1) | (2, 2, 1) | (2, 2, 1) | yes | yes |
| gap 1.1 | (0, 1, 1) | (2, 2, 1) | (2, 2, 1) | yes | yes |
| gap 1.4 to 1.9 | (2, 2, 1) | (2, 2, 1) | (2, 2, 1) | no | no |
| shell, h = 2 | (−89, 7, 1), (−109, 5, 1) | (2, 1, 2) | (2, 1, 2) | yes | yes |
| shell, h = 1.5 | (−175, 1, 1), (−190, 1, 1) | (2, 1, 2) | (2, 1, 2) | yes | yes |
| neck 0.3 | (2, 2, 1) | (1, 1, 1) | (1, 1, 1) | yes | yes |
| neck 0.5, at the origin | (2, 2, 1) | (1, 1, 1) | (1, 1, 1) | yes | yes |
| neck 0.5, moved; 0.7; 0.9 | (1, 1, 1) | (1, 1, 1) | (1, 1, 1) | no | no |
| the necks' shoulders, gaps 0.5 to 0.7 wide | (1, 1, 1) | (1, 1, 1) | (1, 1, 1) | no | no |

Every place that changed was flagged and no place that did not was flagged, 60 places in
all. Where h/2 was wrong too, plate 0.3 and gaps 0.3 and 0.5, it was wrong otherwise than
h, so the place was flagged still. χ alone flagged the same places here; the components
added none.

Time at h/2, a place: 1.3 to 3.7 ms for boxes of 180 to 4800 voxels at h, 44 to 96 ms for
the shell's 15 625 and 32 768; at h/8, 60 ms to 5.7 s. The h/2 samples are eight a voxel of
the box, so their cost follows its volume, not the thin place's area: a thin-walled vessel
whose place is its whole box costs its whole volume at h/2.

## Findings

1. **Comparing the place's box at h and at h/2 found every change here and raised no false
   alarm**, h/2 sampled by the conversion's own sign, without converting again.
2. A place thinner than h/4 can be wrong at h/2 as well, here otherwise than at h. Two
   resolutions wrong alike would hide a change; none was seen.
3. The cost follows the box's volume. Places spanning a large thin-walled shape would need
   a bound, or a region following the thin points rather than their box.

## What this leaves to decide (for the ADR)

- The check: the three invariants of the place's box at h and at h/2, a place flagged where
  they differ, each reported with both sets.
- The region checked: the place's box grown by 2h, as here; with a bound on its voxels at
  h/2 beyond which the place is reported unchecked, or a region of the cells near its
  points.
