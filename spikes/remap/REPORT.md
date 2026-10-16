# Spike: remap (B-001, B-008)

Measured on 2026-10-16 with rustc 1.98.1, rayon 1.12.0 (rayon-core 1.13.0) and proptest
1.12.0, on an Apple M5 (10 cores, 32 GiB). Reproduce from this directory:

```bash
cargo test --release
cargo build --release --bin measure
./target/release/measure remap
./target/release/measure update
```

## Questions

- **B-001**: how is a field not remapped after a topology commit refused?
- **B-008**: where do a new leaf's values come from when a tile splits, and a tile's value
  when a leaf merges into it?

## What was built

The tree is simplified to leaf-sized regions in a `BTreeMap` (`src/core.rs`); the
questions are about the API. One shared core holds the edits, `Remap` and the value remap;
three candidates wrap it:

| Candidate | File | A stale field is |
| :--- | :--- | :--- |
| A. A stamp per topology, checked by each field | `src/generation.rs` | refused at run time, `StaleField` |
| B. The topology owns its fields (`FieldSet`); a commit remaps them all | `src/fieldset.rs` | impossible; a foreign handle is refused at run time |
| C. Topology and fields share a brand (an invariant lifetime) | `src/brand.rs` | a compile error |

`Remap` holds, per new leaf, a `LeafSource` (`Leaf(old)`, `Split(TileSource)`,
`Background`) and, per new tile, a `TileSource` (`Tile(old)`, `Merged(old leaf)`,
`Background`). Each field fills a split leaf with its own tile value and computes a merged
tile with a merge rule it is given: M1 (`uniform`: every value the same bits, else
refused) or M2 (any `Fn(&[T]) -> Option<T>`, here the mean).

## Measurements

### 1. Correctness (`tests/core_props.rs`)

A reference written in the test holds each region's values directly and applies edits to
values. After every commit, three fields (`f32` with M2, `[f32; 3]` and `u8` with M1) must
hold the same bits as the reference, and a refusal must happen in both or neither.

- 4096 cases passed. Counted once by instrumenting the test: 14381 commits applied, 1500
  leaves split from tiles, 265 merges accepted, 319 commits refused by a merge rule, 1632
  refused for an invalid edit.
- A first version passed while exercising almost nothing (3 splits, 1 merge in 1024
  cases): most generated commits held an invalid edit. The generator now drops invalid
  edits in seven commits of eight.
- Same bits at 1, 2 and 8 threads for a remap of 400 leaves into 316 leaves and 134
  merged tiles, followed by a leaf-parallel update.

### 2. A field left stale

| | Forgotten field | Field from elsewhere | Merge refused for one field of several |
| :--- | :--- | :--- | :--- |
| A | `StaleField` on first use (`tests/stale.rs`) | `StaleField` | fields remapped before the refusal are on the new topology; the old topology and fields are intact, so the caller drops the new ones |
| B | cannot happen | `UnknownField` | `commit` changes nothing (asserted in `tests/diffusion.rs`) |
| C | compile error E0521, "borrowed data escapes outside of closure" (doctest) | compile error | as A, inside the closure |

C's compile_fail doctests were checked to fail on the brand itself, and each is paired with
the same code remapping first, which compiles.

### 3. Use (`tests/diffusion.rs`, `tests/sdf.rs`)

Diffusion: five fields (temperature and velocity double-buffered, a flag), updates reading
other fields, two commits that grow, split and merge, and a final commit M1 must refuse.
All three candidates end with the same bits. Spheres: a narrow-band sphere, the union with
a second whose band splits the first one's inside tiles, then a merge of the leaves left
inside; every voxel of every candidate matches the exact clamped distance.

| | Diffusion lines | Sphere lines | Workarounds |
| :--- | :--- | :--- | :--- |
| A | 46 | 15 | none. Every field, scratch buffers included, is remapped by hand at each commit (5 lines) |
| B | 60 | 13 | two. Writing two fields at once has no API (cloned out and back); a read inside the parallel closure returns a `Result` it cannot propagate (`expect` per leaf) |
| C | 65 | 21 | one, structural. A topology cannot be carried across a loop iteration, so each commit nests a closure: a time loop with commits becomes recursion one level per commit, which CODING.md Rule 15 refuses in production code and which grows the stack without bound. A field cannot be stored past its closure without dropping the brand (`into_data`, unchecked) |

Lines are non-blank, non-comment lines of each candidate's wiring.

### 4. Cost (`src/bin/measure.rs`, TESTING.md T5)

Fastest of 30, variants in turn, caches evicted by writing 256 MiB before each run, one
process per comparison, run twice. 10 000 leaves; the edit splits 1000 tiles, merges 1000
leaves and activates 1000.

| Comparison | Variant | Run 1 (µs) | Run 2 (µs) |
| :--- | :--- | ---: | ---: |
| commit and remap 3 fields | A | 9388.9 | 9425.7 |
| | B | 9406.9 | 9521.7 |
| | C | 9419.9 | 9467.7 |
| leaf-parallel update | no check | 417.0 | 416.2 |
| | A, check once | 416.0 | 405.6 |
| | A, check per leaf | 413.7 | 411.8 |

The runs agree within 1.2 % for the remap and 2.5 % for the update; the variants differ by
less than that within either run. No candidate costs measurably more than another.

### 5. Checks shown to fail (TESTING.md T3)

| Break | Failing test |
| :--- | :--- |
| a split leaf filled with the background | `remap_keeps_every_fields_values` |
| a merged tile given the background | `remap_keeps_every_fields_values` |
| A's stamp check always passes | `a_refuses_a_stale_field_at_run_time` |
| C's brand made covariant | the `compile_fail` doctest |
| B's commit sets the shape before remapping | `every_candidate_ends_with_the_same_bits` |

## Findings

1. **B-001.** The type system can refuse a stale field (C), but only by confining each
   topology to a closure, which a solver's time loop cannot carry across commits without
   unbounded recursion. A and B refuse it at run time, at no measurable cost. A needed no
   workaround in either scenario and keeps ARCHITECTURE.md §4.2's separate `Topology` and
   `Field<T>`; B makes a stale field impossible and a refused commit atomic, but needed two
   workarounds and type erasure, and moves fields into the topology.
2. **B-008.** A new leaf's values come from the old leaf, from the tile it was split from
   (each field's own tile value), or from the background. A merged tile's value comes from a
   merge rule each field is given at remap; M1 is one such rule, so one mechanism serves
   both. The rule runs only for merged regions that survive the commit, in leaf order, so
   the error reported does not depend on the thread count. A leaf split or activated in
   the same commit is merged back without the rule, since its values are already uniform.
3. **Not covered.** The spike's tiles are leaf-sized. A tile one or two levels up stands for
   many leaves: splitting it yields child tiles (`TileSource::Tile` serves), but merging
   many leaves or tiles into it needs `Merged` to name several sources and the rule to see
   all their values. This is for the "tiles in full" step (ROADMAP.md §2).
