# Spike: merge (B-010)

Measured on 2026-10-17 with rustc 1.98.1, rayon 1.12.0 (rayon-core 1.13.0) and proptest
1.12.0, on an Apple M5 (10 cores, 32 GiB). Reproduce from this directory:

```bash
cargo test --release
cargo test --release --test pruning -- --nocapture
cargo build --release --bin measure
./target/release/measure commit
./target/release/measure remap
```

## Question

**B-010**: when leaves or tiles merge into a tile above leaf level, how does `Remap` name
the several sources, and what does the merge rule see?

## What was built

A two-level simplification of brodo's tree (`src/lib.rs`): leaf-sized regions, each a leaf,
a leaf-sized tile or nothing, in blocks of `dim`³ regions that may be block tiles instead
(`dim` = 16 is a lower node, 128³ voxels). Sources are found as brodo's commit finds them,
by locating each new leaf and tile in the old topology. The candidates:

| Choice | Candidates |
| :--- | :--- |
| What a block merge accepts | S1: tiles only, so leaves merge first, a commit earlier; S2: leaves and tiles at once |
| An empty region in a merged block | B-a: the merge is refused; B-b: it merges as the background and becomes active |
| How `Remap` names the sources | R1: a `Vec<Part>` per merged tile; R2: one `Vec<Part>`, each merged tile holding a range |
| What the merge rule sees | M-a: every voxel's value; M-b: values with the number of voxels holding each, `&[(T, u32)]` |

A part is an old leaf, an old tile, or a number of empty regions (B-b).

## Measurements

### 1. Correctness (`tests/reference.rs`)

A dense grid over 2³ blocks (dim 2), updated by meaning alone: a commit keeps every leaf
voxel and gives each new tile the rule applied to the old values of the voxels it covers.
Every policy (S1, S2 × B-a, B-b), both representations and four rules (uniform and mean,
each as M-a and M-b) must match it to the bit after every commit, and refuse the same
commits. Values are small integers, so sums and means are exact.

- 128 cases passed, each run under all 32 combinations.
- A first version merged blocks under B-a only twice in all cases: random edits rarely fill
  a block. The generator now fills whole blocks and writes one value everywhere in a third
  of the rounds. Counted once under the mean rule: blocks merged from tiles only (8 to 9
  per policy), from leaves only (11, S2), from leaves and tiles (8, S2), and with empty regions
  under B-b (4 to 39); uniform rules refused 20 to 114 commits per policy.
- Same bits at 1, 2 and 8 threads.
- Under B-a a merged block's parts can still hold empty regions: a region activated in the
  same commit was the background in the old topology. Its values are the background either
  way, so the result is the same.

### 2. Cost (`src/bin/measure.rs`, TESTING.md T5)

Fastest of 20, variants in turn, caches evicted by writing 256 MiB before each run, one
process per comparison, run twice. Eight blocks of 16³ regions merge into block tiles from
every region a tile (`tiles`) or half leaves and half tiles (`mixed`); separately, 32 768
leaves each merge into a leaf-sized tile (`leaves`).

| Comparison | Variant | Run 1 (µs) | Run 2 (µs) | Bytes held |
| :--- | :--- | ---: | ---: | ---: |
| commit, `tiles` | R1 | 6304.1 | 6271.2 | 262 336 |
| | R2 | 6168.5 | 6163.7 | 262 240 |
| commit, `mixed` | R1 | 6377.0 | 6310.9 | 262 336 |
| | R2 | 6234.3 | 6261.2 | 262 240 |
| commit, `leaves` | R1 | 4311.0 | 4349.3 | 1 835 008 |
| | R2 | 3988.8 | 4028.6 | 655 360 |
| remap, `tiles` | uniform M-a | 12681.5 | 11778.9 | |
| | uniform M-b | 51.5 | 52.5 | |
| | mean M-a | 15848.7 | 15856.4 | |
| | mean M-b | 76.1 | 76.8 | |
| remap, `mixed` | uniform M-a | 18412.9 | 17272.0 | |
| | uniform M-b | 18678.8 | 16509.1 | |
| | mean M-a | 22303.1 | 21238.6 | |
| | mean M-b | 25429.7 | 23126.6 | |

- R2 holds 2.8 times less for many small merges (`leaves`) and commits 7 to 8 % faster
  there; for a few large merges the two are the same.
- M-b is 200 to 250 times faster when the parts are tiles (12 681.5 against 51.5 µs
  uniform, 15 848.7 against 76.1 µs mean), since a tile is one entry, not
  512. When half the parts are leaves the two are within the runs' spread: each leaf
  voxel is still one entry.
- The commit's time is mostly the spike's own shape copying, the same for every variant.

### 3. Use (`tests/pruning.rs`)

A sphere SDF (radius 120 voxels, band ±3) built with a leaf in all 17 307 regions it
touches, then pruned: uniform leaves into leaf-sized tiles, wholly inside blocks into block
tiles. Both end with 5 903 leaves, 7 308 leaf-sized tiles and 1 block tile, every sampled
voxel at the clamped exact distance.

| | Commits | Lines of pruning | Time (ms, one run) |
| :--- | ---: | ---: | ---: |
| S1 | 2 | 17 | 17.4 |
| S2 | 1 | 17 | 14.3 |

Under both, the pruning code decides which leaves and blocks are uniform before the
commit: a rule that refuses one tile refuses the whole commit, so the rule serves as a
check, not as a filter.

### 4. Checks shown to fail (TESTING.md T3)

| Break | Failing test |
| :--- | :--- |
| empty regions left out of a merge's parts | `every_candidate_matches_the_dense_grid`, `stepwise_refuses_a_leaf_and_refuse_refuses_an_empty_region` |
| a tile counted as one voxel in M-b | `every_candidate_matches_the_dense_grid` |
| a merged leaf read as the background | `every_candidate_matches_the_dense_grid`, `pruning_ends_the_same_under_s1_and_s2` |
| S1 letting leaves merge | `stepwise_refuses_a_leaf_and_refuse_refuses_an_empty_region` |

## Findings

1. **Sources.** A merged tile's sources are a list of parts: old leaves, old tiles, and,
   if allowed, a count of empty regions. One shared list with a range per tile (R2) holds
   2.8 times less than a list per tile when merges are many and small, and is never worse.
2. **The rule.** Values with voxel counts (M-b) express both the uniform rule and a
   weighted mean, and cost one entry per tile instead of one per voxel: 200 to 250 times
   faster when tiles merge into a larger tile, the same when leaves do.
3. **What merges.** S1 and S2 prune to the same result in as many lines; S2 needs one
   commit instead of two and is the more general, S1's merges being a case of it.
4. **Empty regions.** Both B-a and B-b work. B-b makes regions active that were the
   background, so a merge would change which voxels are active; B-a keeps a merge to
   values only. The spike measured no cost difference.
5. **Not covered.** One level above leaves was built. brodo's upper nodes and root merge
   lower nodes and their tiles the same way, one level up, with more parts; that was not
   measured. A rule refusing one tile refuses the whole commit, which pruning code must
   anticipate.
