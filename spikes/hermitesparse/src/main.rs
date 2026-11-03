//! B-023: the memory and writing time of Hermite data on a sphere, held as brodo's ten dense
//! fields (format version 1) or sparsely, a crossing only on an edge that holds one.
//!
//! The sphere is analytic, of radius `r` voxels about a centre off the lattice: a point's
//! sign is whether it lies within, an edge's crossing is where the sphere meets it, found
//! from the quadratic, its normal the radius there. The topology is a narrow band's, as
//! colino's conversion gives: a leaf where a voxel's centre lies within 3 voxels of the
//! sphere, inside tiles at the largest level whose region lies wholly within it.
//!
//! The sparse layout tried keeps brodo's sign field, and per leaf and axis a mask of the
//! edges with a crossing, 64 bytes, and the crossings in the mask's order, 20 bytes each.
//!
//! Usage: `hermitesparse-spike none|dense|sparse <radius> <out>`; prints its counts and the
//! seconds taken to fill the fields and to write them.

use std::io::Write;
use std::time::Instant;

use brodo::cache::{CacheEntry, to_bytes};
use brodo::field::Field;
use brodo::hermite::{Crossings, Hermite, NO_CROSSING};
use brodo::meta::GridMeta;
use brodo::topology::{LeafNumber, Topology};
use brodo::tree::{Coord, TileLevel};

const BAND: f64 = 3.0;
const CENTRE: [f64; 3] = [0.371, 0.213, 0.137];

struct Sphere {
    r: f64,
}

impl Sphere {
    fn d(&self, p: [f64; 3]) -> f64 {
        let q = [0, 1, 2].map(|k| p[k] - CENTRE[k]);
        (q[0] * q[0] + q[1] * q[1] + q[2] * q[2]).sqrt() - self.r
    }

    fn inside(&self, v: Coord) -> bool {
        self.d(v.map(f64::from)) < 0.0
    }

    /// The crossing on point `v`'s edge along `a`, if its ends differ in sign.
    fn crossing(&self, v: Coord, a: usize) -> Option<(f32, [f32; 3], u32)> {
        let mut w = v;
        w[a] += 1;
        if self.inside(v) == self.inside(w) {
            return None;
        }
        let d = [0, 1, 2].map(|k| f64::from(v[k]) - CENTRE[k]);
        let dd = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
        let root = (d[a] * d[a] - dd + self.r * self.r).max(0.0).sqrt();
        let t = [-d[a] - root, -d[a] + root].into_iter().find(|t| (0.0..=1.0).contains(t))?;
        let mut p = d;
        p[a] += t;
        let n = p.map(|c| c / self.r);
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        Some((t as f32, n.map(|c| (c / len) as f32), 0))
    }
}

/// The band's topology: leaves, and inside tiles at the largest level that fits.
fn topology(s: &Sphere) -> Topology {
    let empty = Topology::new();
    let mut e = empty.edit();
    let reach = (s.r + BAND + 16.0) as i32;
    let blocks = (reach + 7) / 8;
    // A region of edge `n` voxels at `o` lies wholly inside, by its voxel centres, when its
    // farthest centre does.
    let far = |o: Coord, n: i32| {
        let c = [0, 1, 2].map(|k| {
            let (lo, hi) = (f64::from(o[k]) + 0.5, f64::from(o[k] + n) - 0.5);
            if (lo - CENTRE[k]).abs() > (hi - CENTRE[k]).abs() { lo } else { hi }
        });
        s.d(c)
    };
    let mut uppers = std::collections::BTreeSet::new();
    for z in -(reach / 128 + 1)..=reach / 128 {
        for y in -(reach / 128 + 1)..=reach / 128 {
            for x in -(reach / 128 + 1)..=reach / 128 {
                let o = [x * 128, y * 128, z * 128];
                if far(o, 128) < -BAND {
                    e.add_tile(TileLevel::Upper, o).unwrap();
                    uppers.insert(o);
                }
            }
        }
    }
    for z in -blocks..blocks {
        for y in -blocks..blocks {
            for x in -blocks..blocks {
                let o = [x * 8, y * 8, z * 8];
                if uppers.contains(&o.map(|c| c.div_euclid(128) * 128)) {
                    continue;
                }
                let centre = o.map(|c| f64::from(c) + 4.0);
                let dc = s.d(centre);
                if dc.abs() > BAND + 7.0 {
                    if dc < 0.0 {
                        e.add_tile(TileLevel::Lower, o).unwrap();
                    }
                    continue;
                }
                let near = (0..512).any(|i| {
                    let v = [o[0] + i % 8, o[1] + i / 8 % 8, o[2] + i / 64];
                    s.d(v.map(|c| f64::from(c) + 0.5)).abs() <= BAND
                });
                if near {
                    e.activate(o).unwrap();
                } else if far(o, 8) < 0.0 {
                    e.add_tile(TileLevel::Lower, o).unwrap();
                }
            }
        }
    }
    e.commit().unwrap().0
}

fn points(t: &Topology, n: LeafNumber) -> impl Iterator<Item = Coord> {
    let o = t.leaf_origin(n).unwrap();
    (0..512).map(move |i| [o[0] + i % 8, o[1] + i / 8 % 8, o[2] + i / 64])
}

fn signs(s: &Sphere, t: &Topology) -> Field<u8> {
    let mut f = Field::new(t, 0_u8);
    for n in t.leaf_numbers() {
        for (v, p) in f.leaf_mut(t, n).unwrap().iter_mut().zip(points(t, n)) {
            *v = u8::from(s.inside(p));
        }
    }
    for n in t.tile_numbers() {
        f.set_tile(t, n, 1).unwrap();
    }
    f
}

fn dense(s: &Sphere, t: Topology, out: &str) -> (usize, f64, f64) {
    let start = Instant::now();
    let sg = signs(s, &t);
    let (nf, nn, ng) = NO_CROSSING;
    let mut count = 0;
    let axes = [0, 1, 2].map(|a| {
        let mut c = Crossings {
            fraction: Field::new(&t, nf),
            normal: Field::new(&t, nn),
            group: Field::new(&t, ng),
        };
        for n in t.leaf_numbers() {
            let found: Vec<_> = points(&t, n).map(|p| s.crossing(p, a)).collect();
            count += found.iter().flatten().count();
            let each = |x: &Option<(f32, [f32; 3], u32)>| x.unwrap_or(NO_CROSSING);
            for (v, x) in c.fraction.leaf_mut(&t, n).unwrap().iter_mut().zip(&found) {
                *v = each(x).0;
            }
            for (v, x) in c.normal.leaf_mut(&t, n).unwrap().iter_mut().zip(&found) {
                *v = each(x).1;
            }
            for (v, x) in c.group.leaf_mut(&t, n).unwrap().iter_mut().zip(&found) {
                *v = each(x).2;
            }
        }
        c
    });
    let set = Hermite::new(t, sg, axes, Vec::new()).unwrap();
    let filled = start.elapsed().as_secs_f64();
    let start = Instant::now();
    let entries = set.entries("s", &GridMeta::new("s", [1.0; 3], "m").unwrap()).unwrap();
    brodo::cache::write(std::path::Path::new(out), &entries).unwrap();
    (count, filled, start.elapsed().as_secs_f64())
}

/// A crossing as the sparse layout stores it: 20 bytes.
#[derive(Clone, Copy)]
#[repr(C)]
struct Crossing {
    fraction: f32,
    normal: [f32; 3],
    group: u32,
}

/// The sparse layout: the sign field; per leaf and axis a mask of the edges holding a
/// crossing; per leaf where its crossings start; and the crossings, in leaf, axis and
/// place order. Written as the sign grid's cache file followed by the three arrays.
fn sparse(s: &Sphere, t: Topology, out: &str) -> (usize, f64, f64) {
    let start = Instant::now();
    let sg = signs(s, &t);
    let mut masks: Vec<[[u64; 8]; 3]> = Vec::with_capacity(t.leaf_count());
    let mut starts: Vec<u32> = Vec::with_capacity(t.leaf_count() + 1);
    let mut items: Vec<Crossing> = Vec::new();
    for n in t.leaf_numbers() {
        starts.push(items.len() as u32);
        let mut m = [[0_u64; 8]; 3];
        for (a, mask) in m.iter_mut().enumerate() {
            for (i, p) in points(&t, n).enumerate() {
                if let Some((fraction, normal, group)) = s.crossing(p, a) {
                    mask[i / 64] |= 1 << (i % 64);
                    items.push(Crossing { fraction, normal, group });
                }
            }
        }
        masks.push(m);
    }
    starts.push(items.len() as u32);
    let filled = start.elapsed().as_secs_f64();
    let start = Instant::now();
    let meta = GridMeta::new("s", [1.0; 3], "m").unwrap();
    let bytes = to_bytes(&[CacheEntry::new(&meta, &t, &sg).unwrap()]).unwrap();
    let mut f = std::io::BufWriter::new(std::fs::File::create(out).unwrap());
    f.write_all(&bytes).unwrap();
    for m in &masks {
        for w in m.iter().flatten() {
            f.write_all(&w.to_le_bytes()).unwrap();
        }
    }
    for s in &starts {
        f.write_all(&s.to_le_bytes()).unwrap();
    }
    for c in &items {
        f.write_all(&c.fraction.to_le_bytes()).unwrap();
        for x in c.normal {
            f.write_all(&x.to_le_bytes()).unwrap();
        }
        f.write_all(&c.group.to_le_bytes()).unwrap();
    }
    f.into_inner().unwrap().sync_all().unwrap();
    (items.len(), filled, start.elapsed().as_secs_f64())
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (layout, r, out) = (a[1].as_str(), a[2].parse::<f64>().unwrap(), a[3].as_str());
    let s = Sphere { r };
    let start = Instant::now();
    let t = topology(&s);
    let built = start.elapsed().as_secs_f64();
    let (leaves, tiles) = (t.leaf_count(), t.tile_count());
    let (crossings, filled, written) = match layout {
        "dense" => dense(&s, t, out),
        "sparse" => sparse(&s, t, out),
        _ => (0, 0.0, 0.0),
    };
    println!("{leaves}\t{tiles}\t{crossings}\t{built:.1}\t{filled:.1}\t{written:.1}");
}
