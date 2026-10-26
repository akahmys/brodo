//! Spike for B-020's second half (ROADMAP.md §3): can the topology report check each thin
//! place it finds (ADR-0028) by comparing invariants of the solid in the place's box at h
//! and at h/2?
//!
//! Each place's box is grown by 2h and aligned to the h grid's cells, which the h/2 grid
//! splits, so both resolutions see one box. In it the solid is taken three ways: at h, the
//! conversion's voxels (a negative value); at h/2, and at h/8 as the reference, the voxel
//! centres the winding number puts inside (Barill et al.'s approximation at β = 2, the
//! conversion's sign). Of each, three invariants: χ of the union of its closed voxels, its
//! components under 26-connectivity, and the box's other voxels' under 6.
//!
//! A place changed where h's invariants differ from the reference's; it is flagged where
//! they differ from h/2's. Output: one row a place on standard output.

use std::collections::VecDeque;
use std::time::Instant;

use brodo::tree::Coord;
use colino::convert::{Converted, Options, convert};
use colino::mesh::Mesh;
use colino::report::report;
use colino::winding::{Accuracy, Winding};

type P = [f64; 3];

fn par_map<T: Send>(len: usize, f: impl Fn(usize) -> T + Sync) -> Vec<T> {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let chunk = len.div_ceil(threads).max(1);
    let f = &f;
    std::thread::scope(|s| {
        let parts: Vec<_> = (0..len)
            .step_by(chunk)
            .map(|start| s.spawn(move || (start..(start + chunk).min(len)).map(f).collect::<Vec<T>>()))
            .collect();
        parts.into_iter().flat_map(|p| p.join().unwrap_or_default()).collect()
    })
}

/// A dense box of voxels, x fastest.
struct Bitmap {
    n: [usize; 3],
    solid: Vec<bool>,
}

impl Bitmap {
    fn at(&self, c: [i64; 3]) -> bool {
        if (0..3).any(|k| c[k] < 0 || c[k] >= self.n[k] as i64) {
            return false;
        }
        self.solid[c[0] as usize + self.n[0] * (c[1] as usize + self.n[1] * c[2] as usize)]
    }

    /// χ of the union of the closed solid voxels: each lattice element present where a
    /// voxel holding it is solid, counted with the sign of its dimension.
    fn euler(&self) -> i64 {
        let mut chi = 0;
        for z in 0..=self.n[2] as i64 {
            for y in 0..=self.n[1] as i64 {
                for x in 0..=self.n[0] as i64 {
                    for mask in 0..8_u32 {
                        let dim = mask.count_ones();
                        // The voxels holding the element at (x, y, z) spanning the axes in
                        // `mask`: along a spanned axis the voxel at the point, otherwise
                        // either side.
                        let mut present = false;
                        for side in 0..8_u32 {
                            if side & mask != 0 {
                                continue;
                            }
                            let c = [x, y, z];
                            let v = [0, 1, 2].map(|k| c[k] - i64::from(side >> k & 1));
                            if self.at(v) {
                                present = true;
                                break;
                            }
                        }
                        if present {
                            chi += if dim % 2 == 0 { 1 } else { -1 };
                        }
                    }
                }
            }
        }
        chi
    }

    /// Components of the voxels whose solidity is `which`, under 26- or 6-connectivity.
    fn components(&self, which: bool, full: bool) -> i64 {
        let len = self.solid.len();
        let mut seen = vec![false; len];
        let mut count = 0;
        let index = |c: [i64; 3]| c[0] as usize + self.n[0] * (c[1] as usize + self.n[1] * c[2] as usize);
        for start in 0..len {
            if seen[start] || self.solid[start] != which {
                continue;
            }
            count += 1;
            seen[start] = true;
            let mut queue = VecDeque::from([start]);
            while let Some(i) = queue.pop_front() {
                let c = [i % self.n[0], i / self.n[0] % self.n[1], i / (self.n[0] * self.n[1])].map(|x| x as i64);
                for d in 0..27_i64 {
                    let o = [d % 3 - 1, d / 3 % 3 - 1, d / 9 - 1];
                    let steps = o.iter().filter(|x| **x != 0).count();
                    if steps == 0 || (!full && steps > 1) {
                        continue;
                    }
                    let q = [c[0] + o[0], c[1] + o[1], c[2] + o[2]];
                    if (0..3).any(|k| q[k] < 0 || q[k] >= self.n[k] as i64) {
                        continue;
                    }
                    let j = index(q);
                    if !seen[j] && self.solid[j] == which {
                        seen[j] = true;
                        queue.push_back(j);
                    }
                }
            }
        }
        count
    }

    fn invariants(&self) -> [i64; 3] {
        [self.euler(), self.components(true, true), self.components(false, false)]
    }
}

/// The solid in voxels `lo` to `hi` (h's indices) at `h / sub`, by the winding number.
fn sampled(w: &Winding, origin: P, h: f64, lo: Coord, hi: Coord, sub: i32) -> Bitmap {
    let n = [0, 1, 2].map(|k| ((hi[k] - lo[k]) * sub) as usize);
    let step = h / f64::from(sub);
    let solid = par_map(n[0] * n[1] * n[2], |i| {
        let c = [i % n[0], i / n[0] % n[1], i / (n[0] * n[1])];
        let p = [0, 1, 2].map(|k| origin[k] + (f64::from(lo[k] * sub) + c[k] as f64 + 0.5) * step);
        w.number(p, Accuracy::Approximate { beta: 2.0 }) >= 0.5
    });
    Bitmap { n, solid }
}

/// The conversion's solid in voxels `lo` to `hi`.
fn converted(c: &Converted, lo: Coord, hi: Coord) -> Bitmap {
    let n = [0, 1, 2].map(|k| (hi[k] - lo[k]) as usize);
    let mut solid = Vec::with_capacity(n[0] * n[1] * n[2]);
    for z in lo[2]..hi[2] {
        for y in lo[1]..hi[1] {
            for x in lo[0]..hi[0] {
                solid.push(c.field.value_at(&c.topology, [x, y, z]).expect("voxel") < 0.0);
            }
        }
    }
    Bitmap { n, solid }
}

// ---- Shapes ----

fn turn(p: P) -> P {
    let (a, b) = (0.5_f64, 0.3_f64);
    let [x, y, z] = p;
    let (y, z) = (y * a.cos() - z * a.sin(), y * a.sin() + z * a.cos());
    let (x, y) = (x * b.cos() - y * b.sin(), x * b.sin() + y * b.cos());
    [x + 0.31, y + 0.17, z - 0.23]
}

fn boxes(parts: &[(P, P)]) -> Mesh {
    let faces: [[u32; 3]; 12] = [
        [0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4],
        [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5],
    ];
    let (mut vertices, mut triangles) = (Vec::new(), Vec::new());
    for (lo, hi) in parts {
        let base = vertices.len() as u32;
        vertices.extend((0..8).map(|i| turn([0, 1, 2].map(|k| if i >> k & 1 == 1 { hi[k] } else { lo[k] }))));
        triangles.extend(faces.iter().map(|t| t.map(|i| i + base)));
    }
    Mesh::new(vertices, triangles).with_unit(Some("mm".into()))
}

fn sphere(levels: u32, r: f64, inward: bool) -> (Vec<P>, Vec<[u32; 3]>) {
    let mut v: Vec<P> = vec![[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, -1.0]];
    let mut t: Vec<[u32; 3]> = vec![[0, 2, 4], [2, 1, 4], [1, 3, 4], [3, 0, 4], [2, 0, 5], [1, 2, 5], [3, 1, 5], [0, 3, 5]];
    for _ in 0..levels {
        let mut mids = std::collections::BTreeMap::new();
        let mut next = Vec::new();
        for [a, b, c] in t.clone() {
            let mut mid = |u: u32, w: u32| {
                *mids.entry((u.min(w), u.max(w))).or_insert_with(|| {
                    let (p, q) = (v[u as usize], v[w as usize]);
                    let s = [0, 1, 2].map(|k| (p[k] + q[k]) / 2.0);
                    let n = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt();
                    v.push(s.map(|x| x / n));
                    (v.len() - 1) as u32
                })
            };
            let (ab, bc, ca) = (mid(a, b), mid(b, c), mid(c, a));
            next.extend([[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]]);
        }
        t = next;
    }
    let v = v.into_iter().map(|p| p.map(|x| x * r + 0.13)).collect();
    if inward {
        t = t.into_iter().map(|[a, b, c]| [a, c, b]).collect();
    }
    (v, t)
}

fn shell(outer: f64, inner: f64) -> Mesh {
    let (mut v, mut t) = sphere(4, outer, false);
    let (vi, ti) = sphere(4, inner, true);
    let base = v.len() as u32;
    v.extend(vi);
    t.extend(ti.into_iter().map(|x| x.map(|i| i + base)));
    Mesh::new(v, t).with_unit(Some("mm".into()))
}

fn revolved(profile: &[(f64, f64)], m: u32) -> Mesh {
    let (mut vertices, mut rings) = (Vec::new(), Vec::<Vec<u32>>::new());
    for &(r, z) in profile {
        let start = vertices.len() as u32;
        if r == 0.0 {
            vertices.push([0.13, 0.07, z]);
            rings.push(vec![start]);
        } else {
            for j in 0..m {
                let a = std::f64::consts::TAU * f64::from(j) / f64::from(m);
                vertices.push([0.13 + r * a.cos(), 0.07 + r * a.sin(), z]);
            }
            rings.push((start..start + m).collect());
        }
    }
    let mut triangles = Vec::new();
    for w in rings.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        for j in 0..m as usize {
            let k = (j + 1) % m as usize;
            match (a.len(), b.len()) {
                (1, _) => triangles.push([a[0], b[k], b[j]]),
                (_, 1) => triangles.push([a[j], a[k], b[0]]),
                _ => triangles.extend([[a[j], a[k], b[k]], [a[j], b[k], b[j]]]),
            }
        }
    }
    Mesh::new(vertices, triangles).with_unit(Some("mm".into()))
}

fn dumbbell(neck: f64) -> Mesh {
    let lobe = |i: u32, sign: f64| {
        let a = std::f64::consts::PI * f64::from(i) / 24.0;
        (5.0 * a.sin(), sign * (7.0 + 5.0 * a.cos()))
    };
    let mut profile = vec![(0.0, -12.0)];
    profile.extend((1..=20).map(|i| lobe(i, -1.0)));
    profile.extend((-3..=3).map(|z| (neck, f64::from(z))));
    profile.extend((1..=20).rev().map(|i| lobe(i, 1.0)));
    profile.push((0.0, 12.0));
    revolved(&profile, 48)
}

fn main() {
    println!("case\torigin\tplace\tkind\tleast\tbox_voxels\th\thalf\tref\tms_half\tms_ref");
    let only = std::env::var("ONLY").ok();
    let mut cases: Vec<(String, Mesh, f64)> = Vec::new();
    for t in [0.3, 0.5, 0.7, 0.9, 1.1, 1.4, 1.7, 1.9] {
        cases.push((format!("plate_{t}"), boxes(&[([-6.0, -6.0, -t / 2.0], [6.0, 6.0, t / 2.0])]), 1.0));
    }
    for g in [0.3, 0.5, 0.7, 0.9, 1.1, 1.4, 1.7, 1.9] {
        let parts = [([-6.0 - g / 2.0, -4.0, -4.0], [-g / 2.0, 4.0, 4.0]), ([g / 2.0, -4.0, -4.0], [6.0 + g / 2.0, 4.0, 4.0])];
        cases.push((format!("gap_{g}"), boxes(&parts), 1.0));
    }
    cases.push(("shell_h2".into(), shell(20.0, 19.0), 2.0));
    cases.push(("shell_h1.5".into(), shell(20.0, 19.0), 1.5));
    for r in [0.3, 0.5, 0.7, 0.9] {
        cases.push((format!("neck_{r}"), dumbbell(r), 1.0));
    }
    for (name, mesh, h) in cases {
        if only.as_deref().is_some_and(|o| o != name) {
            continue;
        }
        let w = Winding::new(&mesh).expect("mesh");
        for origin in [[0.0, 0.0, 0.0], [0.37, 0.21, 0.43]] {
            let c = convert(&mesh, &Options::new(h).with_origin(origin)).expect("convert");
            let r = report(&mesh, &c, &[]).expect("report");
            for (i, place) in r.thin.iter().enumerate() {
                let index = |p: f64, k: usize| (p - origin[k]) / h;
                #[allow(clippy::cast_possible_truncation)]
                let lo = [0, 1, 2].map(|k| index(place.lo[k], k).floor() as i32 - 2);
                #[allow(clippy::cast_possible_truncation)]
                let hi = [0, 1, 2].map(|k| index(place.hi[k], k).ceil() as i32 + 2);
                let at_h = converted(&c, lo, hi).invariants();
                let t0 = Instant::now();
                let half = sampled(&w, origin, h, lo, hi, 2).invariants();
                let ms_half = t0.elapsed().as_secs_f64() * 1e3;
                let t0 = Instant::now();
                let reference = sampled(&w, origin, h, lo, hi, 8).invariants();
                let ms_ref = t0.elapsed().as_secs_f64() * 1e3;
                let voxels: i64 = (0..3).map(|k| i64::from(hi[k] - lo[k])).product();
                println!(
                    "{name}\t{:?}\t{i}\t{:?}\t{:.3}\t{voxels}\t{at_h:?}\t{half:?}\t{reference:?}\t{ms_half:.1}\t{ms_ref:.1}",
                    origin, place.kind, place.least
                );
            }
        }
    }
}
