//! Spike for B-002 (ROADMAP.md §3): how wide a ghost layer, and does gathering several
//! fields of a leaf at once pay (requests R9 and R10, §4).
//!
//! Each leaf voxel reads 19 values: itself and ±1, ±2, ±3 along each axis, the stencil of
//! a second-order one-sided difference. The variants read the same values in the same
//! order and sum them, so their sums agree bit for bit or one of them is wrong:
//!
//! - `w1_value_at`: brodo's gather (one layer, 10³), `Field::value_at` beyond it;
//! - `w1_accessor`: the same, a `ValueAccessor` beyond it;
//! - `w2_accessor`: a gather two layers wide (12³), an accessor beyond it;
//! - `w3`: a gather three layers wide (14³), every read from it;
//! - `accessor`: no gather, every read through an accessor.
//!
//! For R10, `k` fields of a leaf are gathered one call each through brodo's gather
//! (`fields{k}_apart`), or with the 26 neighbours looked up once and each field copied
//! from them (`fields{k}_shared`); `fields{k}_lookup` times the lookups alone.
//!
//! Grids: a sphere's narrow band (radius 80, band ±3, inside tiles), a block of 16³ leaves,
//! and a slab one leaf thick, 32 × 32 leaves (256² voxels), as a two-dimensional run is.
//! Every run is on one thread, leaf after leaf.
//!
//! Usage: `ghost-spike check` (agreement, and how many reads leave each window), and
//! `ghost-spike` (timed on brodo's harness, at rest; results in `out/bench/ghost/`).

use brodo::accessor::ValueAccessor;
use brodo::field::{Field, LEAF_VOXELS};
use brodo::gather::{GATHER_VOXELS, gather};
use brodo::manifest::Offset;
use brodo::topology::{LeafNumber, Location, Topology};
use brodo::tree::{Coord, TileLevel};

#[path = "../../../crates/brodo/benches/harness/mod.rs"]
mod harness;
use harness::{Failure, Item, repeated};

/// The stencil: the voxel and ±1, ±2, ±3 along each axis.
fn stencil() -> Vec<[i32; 3]> {
    let mut s = vec![[0, 0, 0]];
    for a in 0..3 {
        for k in [1, 2, 3, -1, -2, -3] {
            let mut d = [0; 3];
            d[a] = k;
            s.push(d);
        }
    }
    s
}

/// A value of its own at each voxel, so that a read from the wrong place changes the sum.
fn value(v: Coord) -> f64 {
    let h = v.iter().fold(0x9e37_79b9_u64, |h, c| {
        (h ^ u64::from(c.cast_unsigned())).wrapping_mul(0x0100_0000_01b3)
    });
    (h >> 11) as f64 / (1_u64 << 53) as f64 - 0.5
}

fn fill(t: &Topology, salt: i32) -> Field<f64> {
    let mut f = Field::new(t, 3.0 + f64::from(salt));
    for n in t.leaf_numbers().collect::<Vec<_>>() {
        let voxels: Vec<Coord> = t.leaf_voxels(n).unwrap().collect();
        for (x, v) in f.leaf_mut(t, n).unwrap().iter_mut().zip(voxels) {
            *x = value([v[0] + salt, v[1], v[2]]);
        }
    }
    for n in t.tile_numbers().collect::<Vec<_>>() {
        f.set_tile(t, n, -3.0 - f64::from(salt)).unwrap();
    }
    f
}

struct Grid {
    name: &'static str,
    t: Topology,
    fields: Vec<Field<f64>>,
}

fn grid(name: &'static str, leaves: &[Coord], tiles: &[Coord]) -> Grid {
    let empty = Topology::new();
    let mut e = empty.edit();
    for c in leaves {
        e.activate(*c).unwrap();
    }
    for c in tiles {
        e.add_tile(TileLevel::Lower, *c).unwrap();
    }
    let (t, _) = e.commit().unwrap();
    let fields = (0..6).map(|k| fill(&t, k)).collect();
    Grid { name, t, fields }
}

/// The sphere of radius 80: a leaf where the band ±3 passes, a tile wholly inside it.
fn sphere() -> Grid {
    let (r, lim) = (80.0_f64, 96);
    let (mut leaves, mut tiles) = (Vec::new(), Vec::new());
    for z in (-lim..lim).step_by(8) {
        for y in (-lim..lim).step_by(8) {
            for x in (-lim..lim).step_by(8) {
                let d = |i: i32| {
                    let p = [x + i % 8, y + i / 8 % 8, z + i / 64].map(|c| f64::from(c) + 0.5);
                    (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt() - r
                };
                let ds: Vec<f64> = (0..512).map(d).collect();
                if ds.iter().all(|v| *v <= -3.0) {
                    tiles.push([x, y, z]);
                } else if ds.iter().any(|v| v.abs() < 3.0) {
                    leaves.push([x, y, z]);
                }
            }
        }
    }
    grid("sphere", &leaves, &tiles)
}

fn block(name: &'static str, n: [i32; 3]) -> Grid {
    let mut leaves = Vec::new();
    for z in 0..n[2] {
        for y in 0..n[1] {
            for x in 0..n[0] {
                leaves.push([x * 8, y * 8, z * 8]);
            }
        }
    }
    grid(name, &leaves, &[])
}

/// A gather `w` layers wide, by brodo's gather's method: each voxel of the window from the
/// block of the 27 around the leaf that holds it. `w` is at most 8.
fn gather_wide(t: &Topology, f: &Field<f64>, n: LeafNumber, w: i32, out: &mut Vec<f64>) {
    let sources = sources(t, f, n);
    out.clear();
    for z in -w..8 + w {
        for y in -w..8 + w {
            for x in -w..8 + w {
                let g = [x, y, z];
                out.push(match sources[block_of(g.map(|c| c.div_euclid(8)))] {
                    Source::Leaf(values) => values[local(g)],
                    Source::Value(v) => v,
                });
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Source<'a> {
    Leaf(&'a [f64; LEAF_VOXELS]),
    Value(f64),
}

fn block_of(d: [i32; 3]) -> usize {
    ((d[0] + 1) + 3 * (d[1] + 1) + 9 * (d[2] + 1)) as usize
}

fn local(g: [i32; 3]) -> usize {
    let c = g.map(|x| x.rem_euclid(8) as usize);
    c[0] + 8 * c[1] + 64 * c[2]
}

/// The 26 neighbours' locations, looked up once.
fn row(t: &Topology, n: LeafNumber) -> [Location; 26] {
    *t.neighbours(n).unwrap()
}

/// A field's 27 sources around leaf `n`, from its neighbours' locations.
fn sources_from<'a>(
    t: &Topology,
    f: &'a Field<f64>,
    n: LeafNumber,
    row: &[Location; 26],
) -> [Source<'a>; 27] {
    let mut out = [Source::Value(f.background()); 27];
    out[block_of([0, 0, 0])] = Source::Leaf(f.leaf(t, n).unwrap());
    for d in Offset::all() {
        out[block_of(d.delta())] = match row[d.index()] {
            Location::Leaf(m) => Source::Leaf(f.leaf(t, m).unwrap()),
            Location::Tile(tile, _) => Source::Value(f.tile(t, tile).unwrap()),
            Location::Background => Source::Value(f.background()),
        };
    }
    out
}

fn sources<'a>(t: &Topology, f: &'a Field<f64>, n: LeafNumber) -> [Source<'a>; 27] {
    sources_from(t, f, n, &row(t, n))
}

/// Where a read lands in a window `w` layers wide, or nothing beyond it.
fn at(g: [i32; 3], w: i32) -> Option<usize> {
    let e = 8 + 2 * w;
    g.iter().all(|c| (-w..8 + w).contains(c)).then(|| {
        let l = g.map(|c| c + w);
        (l[0] + e * l[1] + e * e * l[2]) as usize
    })
}

#[derive(Clone, Copy, Debug)]
enum Variant {
    W1ValueAt,
    W1Accessor,
    W2Accessor,
    W3,
    Accessor,
}

const VARIANTS: [(Variant, &str); 5] = [
    (Variant::W1ValueAt, "w1_value_at"),
    (Variant::W1Accessor, "w1_accessor"),
    (Variant::W2Accessor, "w2_accessor"),
    (Variant::W3, "w3"),
    (Variant::Accessor, "accessor"),
];

/// Every leaf voxel's stencil read by the variant, summed in one order.
fn run(g: &Grid, v: Variant, s: &[[i32; 3]]) -> f64 {
    let (t, f) = (&g.t, &g.fields[0]);
    let mut acc = ValueAccessor::new(t, f).unwrap();
    let mut one = [0.0; GATHER_VOXELS];
    let mut wide = Vec::new();
    let mut sum = 0.0;
    for n in t.leaf_numbers() {
        let o = t.leaf_origin(n).unwrap();
        match v {
            Variant::W1ValueAt | Variant::W1Accessor => gather(t, f, n, &mut one).unwrap(),
            Variant::W2Accessor => gather_wide(t, f, n, 2, &mut wide),
            Variant::W3 => gather_wide(t, f, n, 3, &mut wide),
            Variant::Accessor => {}
        }
        for z in 0..8 {
            for y in 0..8 {
                for x in 0..8 {
                    for d in s {
                        let g = [x + d[0], y + d[1], z + d[2]];
                        let world = [o[0] + g[0], o[1] + g[1], o[2] + g[2]];
                        sum += match v {
                            Variant::W1ValueAt => match at(g, 1) {
                                Some(i) => one[i],
                                None => f.value_at(t, world).unwrap(),
                            },
                            Variant::W1Accessor => match at(g, 1) {
                                Some(i) => one[i],
                                None => acc.get(world),
                            },
                            Variant::W2Accessor => match at(g, 2) {
                                Some(i) => wide[i],
                                None => acc.get(world),
                            },
                            Variant::W3 => wide[at(g, 3).unwrap()],
                            Variant::Accessor => acc.get(world),
                        };
                    }
                }
            }
        }
    }
    sum
}

/// `k` fields of each leaf gathered: one brodo gather each, or lookups shared.
fn fields(g: &Grid, k: usize, shared: bool) -> f64 {
    let t = &g.t;
    let mut outs = vec![[0.0; GATHER_VOXELS]; k];
    let mut sum = 0.0;
    for n in t.leaf_numbers() {
        if shared {
            let r = row(t, n);
            for (f, out) in g.fields[..k].iter().zip(&mut outs) {
                let src = sources_from(t, f, n, &r);
                let cells = (-1..=8_i32)
                    .flat_map(|z| (-1..=8).flat_map(move |y| (-1..=8).map(move |x| [x, y, z])));
                for (o, c) in out.iter_mut().zip(cells) {
                    *o = match src[block_of(c.map(|x| x.div_euclid(8)))] {
                        Source::Leaf(values) => values[local(c)],
                        Source::Value(v) => v,
                    };
                }
            }
        } else {
            for (f, out) in g.fields[..k].iter().zip(&mut outs) {
                gather(t, f, n, out).unwrap();
            }
        }
        sum += outs.iter().map(|o| o[555]).sum::<f64>();
    }
    sum
}

/// The lookups alone: each field's 27 sources around each leaf.
fn lookups(g: &Grid, k: usize) -> f64 {
    let t = &g.t;
    let mut sum = 0.0;
    for n in t.leaf_numbers() {
        for f in &g.fields[..k] {
            if let Source::Value(v) = sources(t, f, n)[0] {
                sum += v;
            }
        }
    }
    sum
}

fn grids() -> Vec<Grid> {
    vec![sphere(), block("block", [16, 16, 16]), block("slab", [32, 32, 1])]
}

fn check() {
    let s = stencil();
    for g in grids() {
        let reads = g.t.leaf_count() * LEAF_VOXELS * s.len();
        let (mut out1, mut out2) = (0_usize, 0_usize);
        for z in 0..8 {
            for y in 0..8 {
                for x in 0..8 {
                    for d in &s {
                        let c = [x + d[0], y + d[1], z + d[2]];
                        out1 += usize::from(at(c, 1).is_none());
                        out2 += usize::from(at(c, 2).is_none());
                    }
                }
            }
        }
        let sums: Vec<u64> = VARIANTS.iter().map(|(v, _)| run(&g, *v, &s).to_bits()).collect();
        // brodo's gather against this spike's, one layer wide.
        let (t, f) = (&g.t, &g.fields[0]);
        let (mut a, mut b) = ([0.0; GATHER_VOXELS], Vec::new());
        let same_window = t.leaf_numbers().all(|n| {
            gather(t, f, n, &mut a).unwrap();
            gather_wide(t, f, n, 1, &mut b);
            a.iter().zip(&b).all(|(x, y)| x.to_bits() == y.to_bits())
        });
        let shared: Vec<u64> = [1, 3, 6]
            .iter()
            .flat_map(|k| [fields(&g, *k, false).to_bits(), fields(&g, *k, true).to_bits()])
            .collect();
        println!(
            "{}: {} leaves, {} tiles; {} reads, beyond one layer {} ({:.1} %), beyond two {} ({:.1} %); \
             sums agree: {}; brodo's gather = this one at width 1: {}; fields shared = apart: {}",
            g.name,
            t.leaf_count(),
            t.tile_count(),
            reads,
            out1 * t.leaf_count(),
            100.0 * out1 as f64 / (LEAF_VOXELS * s.len()) as f64,
            out2 * t.leaf_count(),
            100.0 * out2 as f64 / (LEAF_VOXELS * s.len()) as f64,
            sums.iter().all(|x| *x == sums[0]),
            same_window,
            shared.chunks(2).all(|p| p[0] == p[1]),
        );
    }
}

fn items(gs: &[Grid]) -> Vec<Item<'_>> {
    let s = stencil();
    let mut out = Vec::new();
    for g in gs {
        for (v, name) in VARIANTS {
            let s = s.clone();
            out.push(repeated(&format!("{}_{name}", g.name), 1, Box::new(move || Ok(run(g, v, &s)))));
        }
        for k in [1, 3, 6] {
            out.push(repeated(&format!("{}_fields{k}_apart", g.name), 1, Box::new(move || Ok(fields(g, k, false)))));
            out.push(repeated(&format!("{}_fields{k}_shared", g.name), 1, Box::new(move || Ok(fields(g, k, true)))));
            out.push(repeated(&format!("{}_fields{k}_lookup", g.name), 1, Box::new(move || Ok(lookups(g, k)))));
        }
    }
    out
}

fn main() -> Result<(), Failure> {
    if std::env::args().nth(1).as_deref() == Some("check") {
        check();
        return Ok(());
    }
    harness::run(env!("CARGO_MANIFEST_DIR"), "ghost", || Ok(grids()), |gs| items(gs))
}
