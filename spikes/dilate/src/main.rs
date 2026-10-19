//! Spike for B-011 (ROADMAP.md §3): do dilation and erosion need a per-voxel mask?
//!
//! A: no mask. A leaf is active whole; dilating by k adds every background leaf-sized cell
//! with a voxel within k steps of an active cell, eroding removes every cell with a
//! background voxel within k steps. B: a mask per leaf, so the active voxels are exactly
//! the band's and morphology is exact; B is computed here on a dense bitset, which gives
//! its result and size, not its time. The report is `REPORT.md`.
//!
//! Usage: `dilate-spike sizes` (results and sizes), `dilate-spike time <rounds>` (A's time,
//! brodo's own topology edits).

use std::collections::BTreeSet;
use std::time::Instant;

use brodo::analysis::Connectivity;
use brodo::euler::euler;
use brodo::field::Field;
use brodo::topology::Topology;
use brodo::tree::{Coord, TileLevel};

/// The band's half-width in voxels.
const BAND: f64 = 3.0;
/// Voxels along a cell's edge: a leaf.
const CELL: i32 = 8;

/// A shape: its signed distance, its name, and the half-size of a box holding it.
struct Shape {
    name: String,
    half: i32,
    phi: Box<dyn Fn([f64; 3]) -> f64 + Sync>,
}

static SHAPE_FILTER: std::sync::OnceLock<String> = std::sync::OnceLock::new();

fn shapes() -> Vec<Shape> {
    let all = every_shape();
    match SHAPE_FILTER.get() {
        Some(only) => all.into_iter().filter(|s| s.name.starts_with(only.as_str())).collect(),
        None => all,
    }
}

fn every_shape() -> Vec<Shape> {
    let sphere = |r: f64| Shape {
        name: format!("sphere R={r}"),
        half: r as i32 + 4,
        phi: Box::new(move |p: [f64; 3]| (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt() - r),
    };
    vec![
        sphere(80.0),
        sphere(250.0),
        sphere(500.0),
        Shape {
            name: "torus R=120 r=40".into(),
            half: 164,
            phi: Box::new(|p: [f64; 3]| {
                ((p[0] * p[0] + p[1] * p[1]).sqrt() - 120.0).hypot(p[2]) - 40.0
            }),
        },
        Shape {
            name: "plate 128x128x1".into(),
            half: 68,
            phi: Box::new(|p: [f64; 3]| (p[0].abs() - 64.0).max(p[1].abs() - 64.0).max(p[2].abs() - 0.5)),
        },
    ]
}

fn centre(v: Coord) -> [f64; 3] {
    v.map(|c| f64::from(c) + 0.5)
}

/// The cells of a shape's band: leaves (holding a voxel with |φ| < BAND) and tiles (every
/// voxel with φ ≤ −BAND), each by its origin.
struct Cells {
    leaves: BTreeSet<Coord>,
    tiles: BTreeSet<Coord>,
}

fn cells(s: &Shape) -> Cells {
    let (mut leaves, mut tiles) = (BTreeSet::new(), BTreeSet::new());
    let lim = (s.half + CELL) / CELL * CELL;
    for z in (-lim..lim).step_by(8) {
        for y in (-lim..lim).step_by(8) {
            for x in (-lim..lim).step_by(8) {
                // A distance's change over a cell is at most its half-diagonal from the centre.
                let mid = (s.phi)([x, y, z].map(|c| f64::from(c) + 4.0));
                if mid > BAND + 7.0 {
                    continue;
                }
                if mid < -BAND - 7.0 {
                    tiles.insert([x, y, z]);
                    continue;
                }
                let (mut band, mut all_inside) = (false, true);
                for i in 0..512 {
                    let d = (s.phi)(centre([x + i % 8, y + i / 8 % 8, z + i / 64]));
                    band |= d.abs() < BAND;
                    all_inside &= d <= -BAND;
                }
                if band {
                    leaves.insert([x, y, z]);
                } else if all_inside {
                    tiles.insert([x, y, z]);
                }
            }
        }
    }
    Cells { leaves, tiles }
}

/// The cells within k steps of a cell, under a connectivity: a neighbour cell's nearest
/// voxel is one step away along each axis on which it differs, and cells two apart are 9.
fn near(k: u32, conn: Connectivity) -> Vec<[i32; 3]> {
    let mut out = Vec::new();
    for z in -1..=1 {
        for y in -1..=1 {
            for x in -1..=1 {
                let axes = [x, y, z].iter().filter(|c| **c != 0).count() as u32;
                let steps = match conn {
                    Connectivity::TwentySix => axes.min(1),
                    Connectivity::Six => axes,
                };
                if axes > 0 && steps <= k {
                    out.push([x * CELL, y * CELL, z * CELL]);
                }
            }
        }
    }
    out
}

fn add(a: Coord, b: [i32; 3]) -> Coord {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// A without a mask: the leaves after dilating or eroding by k (k ≤ 8).
fn mask_free(c: &Cells, k: u32, conn: Connectivity, dilate: bool) -> BTreeSet<Coord> {
    let offsets = near(k, conn);
    let active = |v: &Coord| c.leaves.contains(v) || c.tiles.contains(v);
    if dilate {
        let mut out = c.leaves.clone();
        for v in c.leaves.iter().chain(&c.tiles) {
            for d in &offsets {
                let w = add(*v, *d);
                if !active(&w) {
                    out.insert(w);
                }
            }
        }
        out
    } else {
        // A leaf, or a tile split down to one, goes when background lies within k.
        c.leaves
            .iter()
            .chain(&c.tiles)
            .filter(|v| c.leaves.contains(*v) && offsets.iter().all(|d| active(&add(**v, *d))))
            .copied()
            .collect()
    }
}

/// A dense bitset over a box, x fastest, rows of 64-bit words.
#[derive(Clone)]
struct Bits {
    lo: i32,
    side: usize,
    row: usize,
    words: Vec<u64>,
}

impl Bits {
    fn new(half: i32) -> Self {
        let side = usize::try_from(2 * half).unwrap().next_multiple_of(64);
        let row = side / 64;
        Bits { lo: -half, side, row, words: vec![0; row * side * side] }
    }
    fn at(&self, v: Coord) -> Option<(usize, u64)> {
        let [x, y, z] = v.map(|c| usize::try_from(c - self.lo).ok().filter(|c| *c < self.side));
        let (x, y, z) = (x?, y?, z?);
        Some((x / 64 + self.row * (y + self.side * z), 1 << (x % 64)))
    }
    fn set(&mut self, v: Coord) {
        let (w, b) = self.at(v).unwrap();
        self.words[w] |= b;
    }
    fn get(&self, v: Coord) -> bool {
        self.at(v).is_some_and(|(w, b)| self.words[w] & b != 0)
    }
    fn count(&self) -> u64 {
        self.words.iter().map(|w| u64::from(w.count_ones())).sum()
    }
    /// Each set bit moved by `s` along x, OR-ed into `out`.
    fn shift_x(&self, s: i32, out: &mut [u64]) {
        let (q, r) = (s.unsigned_abs() as usize / 64, s.unsigned_abs() % 64);
        for (dst, src) in out.chunks_mut(self.row).zip(self.words.chunks(self.row)) {
            for i in 0..self.row {
                let v = if s >= 0 {
                    let a = i.checked_sub(q).map_or(0, |j| src[j] << r);
                    let b = if r == 0 { 0 } else { i.checked_sub(q + 1).map_or(0, |j| src[j] >> (64 - r)) };
                    a | b
                } else {
                    let a = src.get(i + q).map_or(0, |w| w >> r);
                    let b = if r == 0 { 0 } else { src.get(i + q + 1).map_or(0, |w| w << (64 - r)) };
                    a | b
                };
                dst[i] |= v;
            }
        }
    }
    /// Each row moved by `s` rows along y (`stride` = row) or z (`stride` = row × side).
    fn shift_rows(&self, s: i32, stride: usize, out: &mut [u64]) {
        let len = self.words.len();
        let by = s.unsigned_abs() as usize * stride;
        // Rows moved past the box's edge on y wrap into the next z; the margin keeps them 0.
        if s >= 0 {
            for i in by..len {
                out[i] |= self.words[i - by];
            }
        } else {
            for i in 0..len - by {
                out[i] |= self.words[i + by];
            }
        }
    }
    fn dilate(&self, k: u32, conn: Connectivity) -> Bits {
        let mut cur = self.clone();
        let (row, plane) = (self.row, self.row * self.side);
        match conn {
            Connectivity::TwentySix => {
                // A cube of side 2k + 1: one axis at a time.
                for axis in 0..3 {
                    let mut out = cur.words.clone();
                    for s in 1..=k as i32 {
                        for s in [s, -s] {
                            match axis {
                                0 => cur.shift_x(s, &mut out),
                                1 => cur.shift_rows(s, row, &mut out),
                                _ => cur.shift_rows(s, plane, &mut out),
                            }
                        }
                    }
                    cur.words = out;
                }
            }
            Connectivity::Six => {
                for _ in 0..k {
                    let mut out = cur.words.clone();
                    for s in [1, -1] {
                        cur.shift_x(s, &mut out);
                        cur.shift_rows(s, row, &mut out);
                        cur.shift_rows(s, plane, &mut out);
                    }
                    cur.words = out;
                }
            }
        }
        cur
    }
    /// Erosion as the complement of the complement's dilation; the margin is background.
    fn erode(&self, k: u32, conn: Connectivity) -> Bits {
        let mut not = self.clone();
        not.words.iter_mut().for_each(|w| *w = !*w);
        let mut out = not.dilate(k, conn);
        out.words.iter_mut().for_each(|w| *w = !*w);
        out
    }
}

fn voxels_of(cell: Coord) -> impl Iterator<Item = Coord> {
    (0..512).map(move |i| [cell[0] + i % 8, cell[1] + i / 8 % 8, cell[2] + i / 64])
}

/// B's active voxels before morphology: the band's voxels and the tiles'.
fn exact(s: &Shape, c: &Cells, margin: i32) -> Bits {
    let mut b = Bits::new((s.half + CELL) / CELL * CELL + margin);
    for cell in &c.leaves {
        for v in voxels_of(*cell) {
            if (s.phi)(centre(v)).abs() < BAND {
                b.set(v);
            }
        }
    }
    for cell in &c.tiles {
        voxels_of(*cell).for_each(|v| b.set(v));
    }
    b
}

/// The leaf cells B needs: those holding an active voxel, other than tiles.
fn mask_leaves(b: &Bits, c: &Cells) -> u64 {
    let mut cells = BTreeSet::new();
    let lim = b.lo + i32::try_from(b.side).unwrap();
    for z in (b.lo..lim).step_by(8) {
        for y in (b.lo..lim).step_by(8) {
            for x in (b.lo..lim).step_by(8) {
                let cell = [x, y, z];
                if !c.tiles.contains(&cell) && voxels_of(cell).any(|v| b.get(v)) {
                    cells.insert(cell);
                }
            }
        }
    }
    cells.len() as u64
}

/// A's own active voxels before morphology: every voxel of its leaves and tiles.
fn whole(s: &Shape, c: &Cells, margin: i32) -> Bits {
    let mut b = Bits::new((s.half + CELL) / CELL * CELL + margin);
    for cell in c.leaves.iter().chain(&c.tiles) {
        voxels_of(*cell).for_each(|v| b.set(v));
    }
    b
}

fn sizes() {
    println!("shape\tconn\top\tk\tA_leaves\tB_leaves\tA_leaf_voxels\tB_leaf_voxels\tA_over_B_voxels\tA_MiB\tB_MiB\tA_sound\tA_exact_voxels");
    for s in shapes() {
        let c = cells(&s);
        let band_voxels = exact(&s, &c, 0).count() - c.tiles.len() as u64 * 512;
        println!(
            "{}\t-\tnone\t0\t{}\t{}\t{}\t{}\t{:.3}\t-\t-\t-\t-",
            s.name,
            c.leaves.len(),
            c.leaves.len(),
            c.leaves.len() * 512,
            band_voxels,
            (c.leaves.len() * 512) as f64 / band_voxels as f64
        );
        for conn in [Connectivity::Six, Connectivity::TwentySix] {
            for k in [1_u32, 2, 3, 8] {
                let base = exact(&s, &c, 16);
                let own = whole(&s, &c, 16);
                for dilate in [true, false] {
                    let a = mask_free(&c, k, conn, dilate);
                    let b = if dilate { base.dilate(k, conn) } else { base.erode(k, conn) };
                    let tile_voxels = c.tiles.len() as u64 * 512;
                    // On erosion A's tiles stay only where kept as cells; count leaf voxels.
                    let b_leaf_voxels = b.count().saturating_sub(tile_voxels);
                    let b_leaves = mask_leaves(&b, &c);
                    // A is sound against the exact morphology of its own voxels: it holds
                    // every voxel of the exact dilation, and only voxels of the exact erosion.
                    let a_exact = if dilate { own.dilate(k, conn) } else { own.erode(k, conn) };
                    let a_active = |v: Coord| {
                        let cell = v.map(|q| q.div_euclid(CELL) * CELL);
                        a.contains(&cell) || c.tiles.contains(&cell)
                    };
                    let sound = if dilate {
                        let lim = a_exact.lo + i32::try_from(a_exact.side).unwrap();
                        let mut all = true;
                        'z: for z in a_exact.lo..lim {
                            for y in a_exact.lo..lim {
                                for x in a_exact.lo..lim {
                                    if a_exact.get([x, y, z]) && !a_active([x, y, z]) {
                                        all = false;
                                        break 'z;
                                    }
                                }
                            }
                        }
                        all
                    } else {
                        a.iter().all(|cell| voxels_of(*cell).all(|v| a_exact.get(v)))
                    };
                    let a_voxels = a.len() as u64 * 512;
                    println!(
                        "{}\t{:?}\t{}\t{k}\t{}\t{}\t{}\t{}\t{:.3}\t{:.1}\t{:.1}\t{}\t{}",
                        s.name,
                        conn,
                        if dilate { "dilate" } else { "erode" },
                        a.len(),
                        b_leaves,
                        a_voxels,
                        b_leaf_voxels,
                        a_voxels as f64 / b_leaf_voxels.max(1) as f64,
                        (a.len() * 2048) as f64 / 1048576.0,
                        (b_leaves * (2048 + 64)) as f64 / 1048576.0,
                        sound,
                        a_exact.count().saturating_sub(tile_voxels),
                    );
                }
            }
        }
    }
}

/// A's dilation and erosion with brodo's own edits: activating the new leaves, and
/// rebuilding without the removed ones (brodo has no removal yet).
fn time(rounds: usize) {
    println!("shape\tconn\tk\tdilate_ms\terode_ms\tchi_before\tchi_after_dilate");
    for s in shapes() {
        let c = cells(&s);
        let build = |leaves: &BTreeSet<Coord>| {
            let empty = Topology::new();
            let mut e = empty.edit();
            for v in leaves {
                e.activate(*v).unwrap();
            }
            for v in &c.tiles {
                e.add_tile(TileLevel::Lower, *v).unwrap();
            }
            e.commit().unwrap().0
        };
        let t = build(&c.leaves);
        for conn in [Connectivity::Six, Connectivity::TwentySix] {
            for k in [1_u32, 3] {
                let (mut dil, mut ero) = (f64::MAX, f64::MAX);
                let mut dilated = None;
                for _ in 0..rounds {
                    let start = Instant::now();
                    let grown = mask_free(&c, k, conn, true);
                    let mut e = t.edit();
                    for v in grown.difference(&c.leaves) {
                        e.activate(*v).unwrap();
                    }
                    let (d, _) = e.commit().unwrap();
                    dil = dil.min(start.elapsed().as_secs_f64() * 1e3);
                    dilated = Some(d);
                    let start = Instant::now();
                    let kept = mask_free(&c, k, conn, false);
                    let eroded = build(&kept);
                    ero = ero.min(start.elapsed().as_secs_f64() * 1e3);
                    std::hint::black_box(eroded);
                }
                // The solid (φ < 0) is unchanged by dilating: new leaves hold the background.
                let solid = |t: &Topology| {
                    let mut f = Field::new(t, 1.0_f32);
                    for n in t.leaf_numbers().collect::<Vec<_>>() {
                        let at: Vec<Coord> = t.leaf_voxels(n).unwrap().collect();
                        for (v, x) in at.into_iter().zip(f.leaf_mut(t, n).unwrap()) {
                            *x = (s.phi)(centre(v)).clamp(-BAND, BAND) as f32;
                        }
                    }
                    for n in t.tile_numbers().collect::<Vec<_>>() {
                        f.set_tile(t, n, -BAND as f32).unwrap();
                    }
                    euler(t, &f, |v| v < 0.0, conn).unwrap()
                };
                let after = dilated.as_ref().map(solid).unwrap();
                println!("{}\t{:?}\t{k}\t{dil:.2}\t{ero:.2}\t{}\t{after}", s.name, conn, solid(&t));
            }
        }
    }
}

/// The bitset's morphology on cases worked out by hand, across word boundaries too.
fn check() {
    for at in [[0, 0, 0], [63, 5, -7], [64, -1, 30], [-65, 0, 0]] {
        let mut b = Bits::new(100);
        b.set(at);
        let n = |k, conn| b.dilate(k, conn).count();
        assert_eq!(n(1, Connectivity::TwentySix), 27, "{at:?}");
        assert_eq!(n(2, Connectivity::TwentySix), 125, "{at:?}");
        assert_eq!(n(1, Connectivity::Six), 7, "{at:?}");
        assert_eq!(n(2, Connectivity::Six), 25, "{at:?}");
        assert!(b.dilate(1, Connectivity::Six).get([at[0] + 1, at[1], at[2]]), "{at:?}");
        assert!(b.dilate(1, Connectivity::Six).get([at[0], at[1], at[2] - 1]), "{at:?}");
    }
    let mut cube = Bits::new(100);
    for z in 62..65 {
        for y in 0..3 {
            for x in 62..65 {
                cube.set([x, y, z]);
            }
        }
    }
    let e = cube.erode(1, Connectivity::TwentySix);
    assert_eq!(e.count(), 1);
    assert!(e.get([63, 1, 63]));
    assert_eq!(cube.erode(1, Connectivity::Six).count(), 1);
    assert_eq!(near(1, Connectivity::TwentySix).len(), 26);
    assert_eq!(near(1, Connectivity::Six).len(), 6);
    assert_eq!(near(2, Connectivity::Six).len(), 18);
    assert_eq!(near(3, Connectivity::Six).len(), 26);
    println!("check: ok");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Ok(only) = std::env::var("ONLY") {
        SHAPE_FILTER.set(only).unwrap();
    }
    match args.first().map(String::as_str) {
        Some("check") => check(),
        Some("time") => time(args.get(1).and_then(|r| r.parse().ok()).unwrap_or(5)),
        _ => sizes(),
    }
}
