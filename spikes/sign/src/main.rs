//! Spike for B-016 (ROADMAP.md §3): which sign does not depend on a mesh's orientation?
//!
//! Every case is a broken mesh and the mesh it was meant to be. The truth at a point is the
//! intended mesh's exact winding number ≥ 1/2 (Jacobson et al., 2013). Each method signs a
//! dense grid of voxel centres from the broken mesh alone, and the voxels it signs wrongly
//! are counted within the band (3 voxels of the intended mesh) and beyond it, where a wrong
//! sign is a false surface that Hermite data refuses (ADR-0030).
//!
//! Methods:
//! - `W`: the winding number ≥ 1/2, Barill et al.'s approximation at β = 2, colino's sign.
//! - `A`: its magnitude, |w| ≥ 1/2.
//! - `B`: a flood from the grid's boundary along the lattice's edges that cross no
//!   triangle; what it does not reach is inside.
//! - `C`: `B`'s regions, each inside when its mean |w| is ≥ 1/2, the region reaching the
//!   boundary outside.
//! - `D1-k`: parity count along lines in `k` directions, a majority vote, a line with an odd
//!   number of crossings not voting, ties outside (Nooruddin and Turk, 2003, §3.1).
//! - `D2-k`: ray stabbing, inside between a line's first and last crossing, and only where
//!   every direction says so (Nooruddin and Turk, 2003, §3.2).
//!
//! Output: one row a case and method, tab-separated, on standard output.

use std::collections::VecDeque;
use std::time::Instant;

use colino::bvh::Bvh;
use colino::intersect::crossing;
use colino::mesh::Mesh;
use colino::winding::{Accuracy, Winding};

type P = [f64; 3];

const BAND: f64 = 3.0;
const MARGIN: usize = 6;

fn add(a: P, b: P) -> P {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: P, s: f64) -> P {
    a.map(|x| x * s)
}

fn dot(a: P, b: P) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross3(a: P, b: P) -> P {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn unit(a: P) -> P {
    scale(a, 1.0 / dot(a, a).sqrt())
}

/// Runs `f` on 0..len over every core, in order.
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

/// A dense grid of voxel centres over a mesh's box, `MARGIN` voxels beyond it.
struct Grid {
    lo: P,
    h: f64,
    n: [usize; 3],
}

impl Grid {
    fn of(m: &Mesh, h: f64) -> Grid {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for v in &m.vertices {
            for k in 0..3 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
        let n = [0, 1, 2].map(|k| ((hi[k] - lo[k]) / h).ceil() as usize + 2 * MARGIN);
        let lo = [0, 1, 2].map(|k| lo[k] - MARGIN as f64 * h);
        Grid { lo, h, n }
    }

    fn len(&self) -> usize {
        self.n[0] * self.n[1] * self.n[2]
    }

    fn coord(&self, i: usize) -> [usize; 3] {
        [i % self.n[0], i / self.n[0] % self.n[1], i / (self.n[0] * self.n[1])]
    }

    fn index(&self, c: [usize; 3]) -> usize {
        c[0] + self.n[0] * (c[1] + self.n[1] * c[2])
    }

    fn point(&self, i: usize) -> P {
        let c = self.coord(i);
        [0, 1, 2].map(|k| self.lo[k] + (c[k] as f64 + 0.5) * self.h)
    }

    fn on_boundary(&self, i: usize) -> bool {
        let c = self.coord(i);
        (0..3).any(|k| c[k] == 0 || c[k] + 1 == self.n[k])
    }

    fn diagonal(&self) -> f64 {
        self.n.iter().map(|x| (*x as f64 * self.h).powi(2)).sum::<f64>().sqrt()
    }
}

// ---- Shapes, from textbook constructions ----

/// The octahedron split `levels` times onto a sphere of radius `r` about `c`, facing out.
fn sphere(levels: u32, r: f64, c: P) -> Mesh {
    let mut vertices: Vec<P> = vec![
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
    ];
    let mut triangles: Vec<[u32; 3]> = vec![
        [0, 2, 4],
        [2, 1, 4],
        [1, 3, 4],
        [3, 0, 4],
        [2, 0, 5],
        [1, 2, 5],
        [3, 1, 5],
        [0, 3, 5],
    ];
    for _ in 0..levels {
        let mut mids = std::collections::BTreeMap::new();
        let mut next = Vec::with_capacity(triangles.len() * 4);
        for [a, b, cc] in triangles.clone() {
            let mut mid = |u: u32, v: u32| {
                *mids.entry((u.min(v), u.max(v))).or_insert_with(|| {
                    let s = scale(add(vertices[u as usize], vertices[v as usize]), 0.5);
                    vertices.push(unit(s));
                    (vertices.len() - 1) as u32
                })
            };
            let (ab, bc, ca) = (mid(a, b), mid(b, cc), mid(cc, a));
            next.extend([[a, ab, ca], [ab, b, bc], [ca, bc, cc], [ab, bc, ca]]);
        }
        triangles = next;
    }
    let vertices = vertices.into_iter().map(|v| add(scale(v, r), c)).collect();
    Mesh::new(vertices, triangles)
}

/// The octahedron with vertices at ±r on each axis, facing out.
fn octahedron(r: f64) -> Mesh {
    let m = sphere(0, 1.0, [0.0; 3]);
    Mesh::new(m.vertices.iter().map(|v| scale(*v, r)).collect(), m.triangles)
}

/// An axis-aligned box, facing out: corner i has bit k set at the high end of axis k.
fn boxed(lo: P, hi: P) -> Mesh {
    let vertices = (0..8).map(|i| [0, 1, 2].map(|k| if i >> k & 1 == 1 { hi[k] } else { lo[k] })).collect();
    let triangles = vec![
        [0, 2, 3],
        [0, 3, 1],
        [4, 5, 7],
        [4, 7, 6],
        [0, 1, 5],
        [0, 5, 4],
        [2, 6, 7],
        [2, 7, 3],
        [0, 4, 6],
        [0, 6, 2],
        [1, 3, 7],
        [1, 7, 5],
    ];
    Mesh::new(vertices, triangles)
}

/// `m` with the triangles `which` picks turned over.
fn flipped(m: &Mesh, which: impl Fn(usize) -> bool) -> Mesh {
    let t = m.triangles.iter().enumerate().map(|(i, t)| if which(i) { [t[0], t[2], t[1]] } else { *t }).collect();
    Mesh::new(m.vertices.clone(), t)
}

fn without(m: &Mesh, which: impl Fn(&[P; 3]) -> bool) -> Mesh {
    let t = m
        .triangles
        .iter()
        .filter(|t| !which(&t.map(|i| m.vertices[i as usize])))
        .copied()
        .collect();
    Mesh::new(m.vertices.clone(), t)
}

fn joined(a: &Mesh, b: &Mesh) -> Mesh {
    let base = a.vertices.len() as u32;
    let vertices = a.vertices.iter().chain(&b.vertices).copied().collect();
    let triangles =
        a.triangles.iter().copied().chain(b.triangles.iter().map(|t| t.map(|i| i + base))).collect();
    Mesh::new(vertices, triangles)
}

/// A fixed pseudo-random choice of a fraction `p` of `n` items.
fn chosen(n: usize, p: f64, seed: u64) -> Vec<bool> {
    let mut g = seed;
    (0..n)
        .map(|_| {
            g = g.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            ((g >> 11) as f64 / (1_u64 << 53) as f64) < p
        })
        .collect()
}

struct Case {
    name: &'static str,
    broken: Mesh,
    intended: Mesh,
    h: f64,
}

fn cases() -> Vec<Case> {
    let c = [0.13, 0.07, -0.11];
    let s = sphere(4, 10.0, c);
    let n = s.triangles.len();
    let cube = boxed([-6.3; 3], [6.3; 3]);
    // The cube's +z face: corners 4 to 7.
    let face_in = flipped(&cube, |i| (2..4).contains(&i));
    let few = chosen(n, 0.1, 1);
    let half = chosen(n, 0.5, 2);
    // A hole three voxels across, of radius 1.5 at h = 0.5.
    let z = (10.0_f64.powi(2) - 1.5_f64.powi(2)).sqrt() + c[2];
    let a = boxed([-6.0, -3.0, -3.0], [2.0, 3.0, 3.0]);
    let b = boxed([-2.0, -4.0, -2.0], [6.0, 2.0, 4.0]);
    let inner = sphere(4, 6.0, c);
    let hollow = joined(&s, &flipped(&inner, |_| true));
    vec![
        Case { name: "sphere", broken: s.clone(), intended: s.clone(), h: 0.5 },
        Case { name: "cube_face_in", broken: face_in, intended: cube.clone(), h: 0.5 },
        Case { name: "cube_all_in", broken: flipped(&cube, |_| true), intended: cube.clone(), h: 0.5 },
        Case { name: "sphere_flip10", broken: flipped(&s, |i| few[i]), intended: s.clone(), h: 0.5 },
        Case { name: "sphere_flip50", broken: flipped(&s, |i| half[i]), intended: s.clone(), h: 0.5 },
        Case { name: "sphere_hole1", broken: without(&s, |t| *t == s.triangles[0].map(|i| s.vertices[i as usize])), intended: s.clone(), h: 0.5 },
        Case { name: "sphere_hole3", broken: without(&s, |t| t.iter().all(|p| p[2] > z)), intended: s.clone(), h: 0.5 },
        Case { name: "sphere_double", broken: joined(&s, &s), intended: s.clone(), h: 0.5 },
        Case { name: "boxes_overlap", broken: joined(&a, &b), intended: joined(&a, &b), h: 0.5 },
        Case { name: "boxes_overlap_face_in", broken: joined(&flipped(&a, |i| (2..4).contains(&i)), &b), intended: joined(&a, &b), h: 0.5 },
        Case { name: "hollow", broken: hollow.clone(), intended: hollow.clone(), h: 0.5 },
        // Vertices on the lines through voxel centres: x = 0 is a centre line when the
        // mesh's least x is -6.25 and h = 0.5.
        Case { name: "octa_lattice", broken: octahedron(6.25), intended: octahedron(6.25), h: 0.5 },
        Case { name: "octa_lattice_flip", broken: flipped(&octahedron(6.25), |i| i % 3 == 0), intended: octahedron(6.25), h: 0.5 },
        // Held out: added after E was written from the cases above.
        Case { name: "sphere_double_offset", broken: joined(&s, &sphere(4, 10.02, c)), intended: s.clone(), h: 0.5 },
        Case { name: "boxes_touching", broken: joined(&boxed([-6.0, -3.0, -3.0], [0.0, 3.0, 3.0]), &boxed([0.0, -3.0, -3.0], [6.0, 3.0, 3.0])), intended: boxed([-6.0, -3.0, -3.0], [6.0, 3.0, 3.0]), h: 0.5 },
        Case { name: "hollow_flip10", broken: flipped(&hollow, |i| chosen(hollow.triangles.len(), 0.1, 3)[i]), intended: hollow, h: 0.5 },
        Case { name: "hole1_flip10", broken: flipped(&without(&s, |t| *t == s.triangles[0].map(|i| s.vertices[i as usize])), |i| chosen(n, 0.1, 4)[i]), intended: s.clone(), h: 0.5 },
    ]
}

// ---- Methods ----

fn winding(m: &Mesh, g: &Grid, accuracy: Accuracy) -> Vec<f64> {
    let w = Winding::new(m).expect("mesh");
    par_map(g.len(), |i| w.number(g.point(i), accuracy))
}

/// Whether the lattice edge from voxel `i` to its +axis neighbour crosses a triangle.
fn blocked(m: &Mesh, g: &Grid) -> [Vec<bool>; 3] {
    let bvh = Bvh::new(m).expect("mesh");
    [0, 1, 2].map(|axis| {
        par_map(g.len(), |i| {
            let c = g.coord(i);
            if c[axis] + 1 >= g.n[axis] {
                return true;
            }
            let (a, b) = (g.point(i), add(g.point(i), { let mut d = [0.0; 3]; d[axis] = g.h; d }));
            bvh.near_segment(a, b).into_iter().any(|t| bvh.corners(t).is_some_and(|t| crossing(a, b, t).is_some()))
        })
    })
}

/// Each voxel's region along unblocked edges, numbered from 0.
fn regions(g: &Grid, blocked: &[Vec<bool>; 3]) -> (Vec<u32>, u32) {
    let mut region = vec![u32::MAX; g.len()];
    let mut count = 0;
    for start in 0..g.len() {
        if region[start] != u32::MAX {
            continue;
        }
        region[start] = count;
        let mut queue = VecDeque::from([start]);
        while let Some(i) = queue.pop_front() {
            let c = g.coord(i);
            for axis in 0..3 {
                let mut up = c;
                up[axis] += 1;
                if !blocked[axis][i] && up[axis] < g.n[axis] {
                    let j = g.index(up);
                    if region[j] == u32::MAX {
                        region[j] = count;
                        queue.push_back(j);
                    }
                }
                if c[axis] > 0 {
                    let mut down = c;
                    down[axis] -= 1;
                    let j = g.index(down);
                    if !blocked[axis][j] && region[j] == u32::MAX {
                        region[j] = count;
                        queue.push_back(j);
                    }
                }
            }
        }
        count += 1;
    }
    (region, count)
}

/// The three axes, then the ten directions of an icosahedron's face normals, up to sign.
fn directions(k: usize) -> Vec<P> {
    let phi = (1.0 + 5.0_f64.sqrt()) / 2.0;
    let q = 1.0 / phi;
    let all = [
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 1.0, 1.0],
        [1.0, 1.0, -1.0],
        [1.0, -1.0, 1.0],
        [-1.0, 1.0, 1.0],
        [0.0, q, phi],
        [0.0, q, -phi],
        [q, phi, 0.0],
        [q, -phi, 0.0],
        [phi, 0.0, q],
        [phi, 0.0, -q],
    ];
    all.iter().take(k).map(|d| unit(*d)).collect()
}

/// For one direction, each voxel's crossings along the line through it: in all, and beyond
/// the voxel. Triangles are binned by their projection on the plane across the direction.
fn line_crossings(m: &Mesh, g: &Grid, d: P) -> Vec<(u32, u32)> {
    let dedup = std::env::var("DEDUP").is_ok_and(|v| v == "1");
    let helper = if d[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let u = unit(cross3(d, helper));
    let v = cross3(d, u);
    let tris: Vec<[P; 3]> = m.triangles.iter().map(|t| t.map(|i| m.vertices[i as usize])).collect();
    let proj = |p: P| (dot(p, u), dot(p, v));
    let (mut ulo, mut vlo, mut uhi, mut vhi) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for t in &tris {
        for p in t {
            let (a, b) = proj(*p);
            (ulo, vlo, uhi, vhi) = (ulo.min(a), vlo.min(b), uhi.max(a), vhi.max(b));
        }
    }
    let cell = g.h;
    let nu = ((uhi - ulo) / cell).floor() as usize + 1;
    let nv = ((vhi - vlo) / cell).floor() as usize + 1;
    let mut bins: Vec<Vec<u32>> = vec![Vec::new(); nu * nv];
    let eps = 1e-9 * (uhi - ulo).abs().max(vhi - vlo).max(1.0);
    for (i, t) in tris.iter().enumerate() {
        let ps = t.map(proj);
        let (a0, a1) = ps.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), p| (l.min(p.0), h.max(p.0)));
        let (b0, b1) = ps.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), p| (l.min(p.1), h.max(p.1)));
        let iu0 = (((a0 - eps - ulo) / cell).floor().max(0.0)) as usize;
        let iu1 = (((a1 + eps - ulo) / cell).floor() as usize).min(nu - 1);
        let iv0 = (((b0 - eps - vlo) / cell).floor().max(0.0)) as usize;
        let iv1 = (((b1 + eps - vlo) / cell).floor() as usize).min(nv - 1);
        for x in iu0..=iu1 {
            for y in iv0..=iv1 {
                bins[x + nu * y].push(i as u32);
            }
        }
    }
    let reach = g.diagonal() * 2.0;
    let tie = std::env::var("TIE").unwrap_or_else(|_| if dedup { "dedup".into() } else { "colino".into() });
    // The fixed offset across the line, as fractions of h.
    let shift = add(scale(u, 0.003_141 * g.h), scale(v, 0.002_718 * g.h));
    let axis = (0..3).find(|k| d[*k] == 1.0);
    par_map(g.len(), |i| {
        let p0 = g.point(i);
        let p = if tie == "offset" { add(p0, shift) } else { p0 };
        let (a, b) = proj(p);
        let (x, y) = (((a - ulo) / cell).floor(), ((b - vlo) / cell).floor());
        if x < 0.0 || y < 0.0 || x as usize >= nu || y as usize >= nv {
            return (0, 0);
        }
        let bin = &bins[x as usize + nu * y as usize];
        if tie == "sos" {
            let Some(kz) = axis else { return (0, 0) };
            let (mut all, mut beyond) = (0, 0);
            for t in bin {
                if let Some(z) = sos_crossing(p, kz, tris[*t as usize]) {
                    all += 1;
                    beyond += u32::from(z >= 0.0);
                }
            }
            return (all, beyond);
        }
        let (start, end) = (add(p, scale(d, -reach)), add(p, scale(d, reach)));
        // `crossing` hits both triangles of an edge the line passes exactly on; with
        // `TIE=dedup` crossings at one fraction count once.
        let mut at: Vec<f64> = bin.iter().filter_map(|t| crossing(start, end, tris[*t as usize])).collect();
        if tie == "dedup" {
            at.sort_by(f64::total_cmp);
            at.dedup_by(|a, b| a.to_bits() == b.to_bits());
        }
        let beyond = at.iter().filter(|s| **s >= 0.5).count() as u32;
        (at.len() as u32, beyond)
    })
}

/// `a·b − c·d`, of the right sign: equal rounded products are decided by their rounding
/// errors, each exact through a fused multiply-add.
fn exact(a: f64, b: f64, c: f64, d: f64) -> f64 {
    let (x, y) = (a * b, c * d);
    if x.to_bits() != y.to_bits() {
        return x - y;
    }
    a.mul_add(b, -x) - c.mul_add(d, -y)
}

/// Where the line through `p` along axis `kz` crosses `t`, as a coordinate along the axis
/// from `p`. An edge function exactly 0 takes the sign it has at the point moved by
/// (ε, ε²) in the plane across the line, so that a line on a shared edge or a corner is
/// inside exactly the triangles the moved point is inside.
fn sos_crossing(p: P, kz: usize, t: [P; 3]) -> Option<f64> {
    let (kx, ky) = ((kz + 1) % 3, (kz + 2) % 3);
    let q = t.map(|c| [c[kx] - p[kx], c[ky] - p[ky], c[kz] - p[kz]]);
    // The edge from q[i] to q[j], at the origin: (qj − qi) × (o − qi).
    let side = |i: usize, j: usize| {
        let (a, b) = (q[i], q[j]);
        let e = exact(a[0], b[1], a[1], b[0]);
        if e != 0.0 {
            return e.signum();
        }
        // d/dε of (b − a) × (δ − a) at δ = (ε, ε²): −(b_y − a_y), then (b_x − a_x).
        let dy = b[1] - a[1];
        if dy != 0.0 {
            return -dy.signum();
        }
        (b[0] - a[0]).signum()
    };
    let (s0, s1, s2) = (side(0, 1), side(1, 2), side(2, 0));
    if !(s0 == s1 && s1 == s2) || s0 == 0.0 {
        return None;
    }
    // The crossing's height by barycentric weights, the signed areas opposite each corner.
    let area = |i: usize, j: usize| exact(q[i][0], q[j][1], q[i][1], q[j][0]);
    let (w2, w0, w1) = (area(0, 1), area(1, 2), area(2, 0));
    let det = w0 + w1 + w2;
    if det == 0.0 {
        return None;
    }
    Some((w0 * q[0][2] + w1 * q[1][2] + w2 * q[2][2]) / det)
}

fn parity(m: &Mesh, g: &Grid, k: usize) -> Vec<bool> {
    let mut votes = vec![0_i32; g.len()];
    for d in directions(k) {
        for (vote, (all, beyond)) in votes.iter_mut().zip(line_crossings(m, g, d)) {
            if all % 2 == 0 {
                *vote += if beyond % 2 == 1 { 1 } else { -1 };
            }
        }
    }
    votes.into_iter().map(|v| v > 0).collect()
}

fn stabbing(m: &Mesh, g: &Grid, k: usize) -> Vec<bool> {
    let mut inside = vec![true; g.len()];
    for d in directions(k) {
        for (x, (all, beyond)) in inside.iter_mut().zip(line_crossings(m, g, d)) {
            *x &= beyond >= 1 && all - beyond >= 1;
        }
    }
    inside
}

fn main() {
    println!("case\tmethod\tvoxels\tband\tbeyond\twrong_band\twrong_beyond\tms");
    let only = std::env::var("ONLY").ok();
    for case in cases() {
        if only.as_deref().is_some_and(|o| o != case.name) {
            continue;
        }
        let g = Grid::of(&case.intended, case.h);
        let truth: Vec<bool> = winding(&case.intended, &g, Accuracy::Exact).into_iter().map(|w| w >= 0.5).collect();
        let bvh = Bvh::new(&case.intended).expect("mesh");
        let near: Vec<bool> = par_map(g.len(), |i| bvh.nearest(g.point(i)).is_some_and(|(d, _)| d <= BAND * g.h));
        let band = near.iter().filter(|x| **x).count();
        let report = |method: &str, signs: Vec<bool>, ms: f64| {
            let (mut wb, mut wf) = (0, 0);
            for i in 0..g.len() {
                if signs[i] != truth[i] {
                    if near[i] { wb += 1 } else { wf += 1 }
                }
            }
            println!("{}\t{method}\t{}\t{band}\t{}\t{wb}\t{wf}\t{ms:.1}", case.name, g.len(), g.len() - band);
            if std::env::var("SHOW").ok().as_deref() == Some(method) {
                for i in (0..g.len()).filter(|i| signs[*i] != truth[*i]).take(30) {
                    eprintln!("wrong {method} {:?} {:?} truth {}", g.coord(i), g.point(i), truth[i]);
                }
            }
        };
        let m = &case.broken;
        let approx = Accuracy::Approximate { beta: 2.0 };
        let t = Instant::now();
        let w = winding(m, &g, approx);
        let w_ms = t.elapsed().as_secs_f64() * 1e3;
        report("W", w.iter().map(|x| *x >= 0.5).collect(), w_ms);
        report("A", w.iter().map(|x| x.abs() >= 0.5).collect(), w_ms);
        let t = Instant::now();
        let bl = blocked(m, &g);
        let (region, count) = regions(&g, &bl);
        let b_ms = t.elapsed().as_secs_f64() * 1e3;
        let outside: Vec<bool> = {
            let mut o = vec![false; count as usize];
            for i in 0..g.len() {
                if g.on_boundary(i) {
                    o[region[i] as usize] = true;
                }
            }
            o
        };
        report("B", region.iter().map(|r| !outside[*r as usize]).collect(), b_ms);
        let mut sum = vec![(0.0_f64, 0_u64); count as usize];
        for i in 0..g.len() {
            let s = &mut sum[region[i] as usize];
            s.0 += w[i].abs();
            s.1 += 1;
        }
        let c_inside: Vec<bool> =
            (0..count as usize).map(|r| !outside[r] && sum[r].0 / sum[r].1 as f64 >= 0.5).collect();
        report("C", region.iter().map(|r| c_inside[*r as usize]).collect(), b_ms + w_ms);
        // E: a region's depth, the fewest walls between it and a region reaching the
        // boundary; odd is inside, and an even region is inside where |w| ≥ 1/2.
        let t = Instant::now();
        let mut next: Vec<Vec<u32>> = vec![Vec::new(); count as usize];
        for axis in 0..3 {
            for i in 0..g.len() {
                let c = g.coord(i);
                if bl[axis][i] && c[axis] + 1 < g.n[axis] {
                    let mut up = c;
                    up[axis] += 1;
                    let (a, b) = (region[i], region[g.index(up)]);
                    if a != b {
                        next[a as usize].push(b);
                        next[b as usize].push(a);
                    }
                }
            }
        }
        let mut depth = vec![u32::MAX; count as usize];
        let mut queue = VecDeque::new();
        for r in 0..count as usize {
            if outside[r] {
                depth[r] = 0;
                queue.push_back(r);
            }
        }
        while let Some(r) = queue.pop_front() {
            for q in next[r].clone() {
                if depth[q as usize] == u32::MAX {
                    depth[q as usize] = depth[r] + 1;
                    queue.push_back(q as usize);
                }
            }
        }
        let e_ms = t.elapsed().as_secs_f64() * 1e3 + b_ms + w_ms;
        report(
            "E",
            (0..g.len()).map(|i| depth[region[i] as usize] % 2 == 1 || w[i].abs() >= 0.5).collect(),
            e_ms,
        );
        for k in [3, 13] {
            let t = Instant::now();
            let s = parity(m, &g, k);
            report(&format!("D1-{k}"), s, t.elapsed().as_secs_f64() * 1e3);
            let t = Instant::now();
            let s = stabbing(m, &g, k);
            report(&format!("D2-{k}"), s, t.elapsed().as_secs_f64() * 1e3);
        }
    }
}
