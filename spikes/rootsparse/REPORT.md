# Spike: rootsparse (B-025, issue 6)

Measured on 2026-11-11 with rustc 1.98.1 and proptest 1.12.0, on an Apple M5, against
brodo at `3be0a13`. Reproduce from this directory:

```bash
cargo test --release
cargo build --release --bin measure
./target/release/measure
```

## Question

`analysis::label` finds the root's empty regions on a grid compressed along each axis
around the root's keys, and refuses as `TooSpread` when that grid would pass 2²² cells.
Each lone key place adds four intervals along an axis, so some 40 keys spread along every
axis reach it: a topology of a few leaves far apart is refused though it is small. Can the
root's empty regions be numbered without the dense grid, giving the walk the same units
and joins, on the layouts the dense grid accepts, and at what cost on those it refuses?

## What was built

`src/lib.rs` takes the root's keys as root-region coordinates and answers what the
labelling walk asks of the root: what a region is (the outside, a key's region, or an
empty region and its unit) and which empty units are joined. Joins between a key's region
and what touches it are made from what each neighbour is, so the two forms give the walk
the same joins if they agree on that.

- **Dense** is `analysis.rs`'s `RootGrid` and `Walk::root`, taken over: the compressed
  axes, every cell of their product visited in z, y, x order, a unit for each empty cell
  in the order the walk makes them, joined across every touching pair, the cells kept in
  a `BTreeMap`, and the same 2²² refusal.
- **Sparse** holds the empty space inside the keys' bounding box, grown by one region, as
  boxes: each run of keyless z is one slab; in each keyed z, each run of keyless y is one
  row across the box; in each keyed row, each run of x between its keys is one box. One
  unit a box, made in that order. Boxes are joined where they touch under the
  connectivity, found by walking the rows of one slab and of the next in order; a box on
  the rim joins the outside, which every region beyond the rim is. A region is found by
  three binary searches, slab, row and run.

The boxes are O(K) for K keys: at most 2 slabs a keyed z, 2 rows a keyed y within it, and
one run more than the keys in a keyed row.

## Measurements

### 1. The two agree (`tests/agree.rs`)

On every layout the dense form accepts, each region probed has the same kind under both,
and the empty components correspond one to one. The probes are both ends of every dense
cell, which meet every dense cell and every sparse box, and every key's neighbours.
Layouts, 400 cases each and both connectivities:

- up to 40 keys among 8³ places
- up to 64 keys among 4³, where many pockets form
- a 6³ block of keys with up to 80 holes, pockets touching only at an edge or a corner
- up to 12 keys among 80³
- a hollow cube, whose inside is one component apart from the outside

The empty components come in the same order under both forms in all 4000 layouts of up to
60 keys among 4³ tried. That order is what `label` numbers components by, so a component
of only empty regions keeps its number.

The tests were shown to fail when broken:

- With the diagonal tolerance between rows set to none, the crowded and holed layouts
  failed under 26-connectivity.
- With rows of one slab not joined, the hollow cube failed.
- With the rim joined to the outside only on the low x face, the tests did not fail. That
  break changes nothing: every row's first box lies on that face.

### 2. Time and memory (`src/bin/measure.rs`)

Each time is the median of five runs; each memory figure is the peak heap the first run
adds. `label` is brodo's own, on a topology with one leaf at each key's origin and a field
of one value meeting the condition. Before timing, the run asserts that the dense form
refuses exactly where `label` returns `TooSpread`, and that the topology holds one leaf a
key.

Blocks and a hollow cube, keys side by side:

| Layout | Keys | Conn | Dense cells | Dense ms | Sparse boxes | Sparse ms | `label` ms | `label` MiB |
| :--- | ---: | :--- | ---: | ---: | ---: | ---: | ---: | ---: |
| one key | 1 | 6 | 125 | 0.005 | 6 | 0.001 | 0.9 | 0.82 |
| block 4³ | 64 | 26 | 512 | 0.165 | 42 | 0.008 | 204 | 26.8 |
| block 8³ | 512 | 6 | 1728 | 0.132 | 146 | 0.020 | 673 | 208 |
| block 8³ | 512 | 26 | 1728 | 0.479 | 146 | 0.019 | 1951 | 210 |
| shell 12³ | 728 | 26 | 4096 | 1.871 | 414 | 0.036 | 2584 | 210 |

Scattered keys, uniformly random over 2¹⁹ root regions an axis, and keys along a diagonal
three regions apart:

| Layout | Keys | Conn | Dense cells | Dense ms | Dense MiB | Sparse ms | Sparse MiB | `label` ms | `label` MiB |
| :--- | ---: | :--- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| scattered | 10 | 6 | 68 921 | 14.9 | 3.06 | 0.003 | 0.01 | 25.6 | 6.16 |
| scattered | 20 | 26 | 531 441 | 407 | 26.1 | 0.005 | 0.02 | 509 | 37.2 |
| scattered | 40 | 6 | 4 173 281 | 1349 | 220 | 0.012 | 0.04 | 1442 | 257 |
| scattered | 40 | 26 | 4 173 281 | 3736 | 220 | 0.009 | 0.04 | 4195 | 257 |
| diagonal | 50 | 26 | 3 511 808 | 3248 | 191 | 0.014 | 0.05 | 3741 | 227 |
| scattered | 50 | either | refused | | | 0.011 | 0.05 | `TooSpread` | |
| scattered | 1000 | either | refused | | | 0.24 | 0.93 | `TooSpread` | |
| scattered | 10 000 | either | refused | | | 2.8 | 9.7 | `TooSpread` | |
| scattered | 100 000 | 26 | refused | | | 45.8 | 88.4 | `TooSpread` | |

The whole table, both connectivities and every count, is the program's output.

## Findings

1. **The sparse form gives the same units and joins on every layout tried that the dense
   form accepts, and the same order of empty components**, so `label`'s numbers would not
   change.
2. **For scattered leaves, the dense grid is nearly all of `label`'s cost.** At 40
   scattered keys it is 94% of the time under 6-connectivity and 89% under 26, and 86% of
   the memory. The sparse form takes 0.01 ms and 0.04 MiB, a hundred thousand times less
   time.
3. **For keys side by side, the dense grid costs little and the sparse form less.** Both
   are under 2 ms where `label` takes seconds. Nothing there gets slower.
4. **The sparse form has no limit of its own.** Its boxes grow as O(K) and its build as
   O(K log K): 100 000 scattered keys take 46 ms and 88 MiB.
5. **The next limit is each lone key's upper node, not the root.** A key holding one leaf
   costs `label` about 1.3 ms and 0.4 MiB under 6-connectivity, and 3.8 ms under 26: a
   block of 512 such keys takes 0.67 s and 2.0 s, and 208 MiB. This cost is the units and
   joins for the upper and lower nodes' empty places, outside the root. With the sparse
   root, 1000 scattered leaves would take about 1.3 s to 3.8 s and 0.4 GiB, and 10 000
   about ten times that. These are estimates from the per-key cost, not measured: `label`
   refuses those layouts today.

## What this leaves to decide

- Whether `label` finds the root's empty regions by the sparse form, and so stops
  refusing anything as `TooSpread`. The variant is public and `AnalysisError` is
  `#[non_exhaustive]`, so it can stay, never returned and documented as such, until a
  MAJOR removes it.
- Whether the cost of Finding 5 is recorded as a limit, or as a request for a sparse
  form of a lone upper node's empty places.
