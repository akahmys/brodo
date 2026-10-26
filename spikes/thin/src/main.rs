//! Spike for B-020 (ROADMAP.md §3): how does colino find places thinner, or gaps narrower,
//! than two voxels, from the mesh, before converting it?
//!
//! Points are sampled on every triangle, about h/2 apart. At each, with the triangle's
//! normal n taken either way, so that orientation does not matter:
//!
//! - `A`, rays: along +n and −n, the nearest hit on each side beyond the point; a side whose
//!   hit's midpoint is inside is a wall that thick, otherwise a gap that wide.
//! - `B`, a shrinking ball: on each side, the largest ball tangent at the point, along ±n,
//!   holding no point of the mesh, found by shrinking from a cap: the nearest point y of the
//!   mesh to the ball's centre, while nearer than the radius, sets the radius to that of the
//!   ball tangent at the point through y, |y − x|² / (2 n·(y − x)). Its diameter is the
//!   wall's thickness if its centre is inside, otherwise the gap's width. The idea is Ma,
//!   Bae and Choi's (The Visual Computer 28(1), 2012), from their abstract; the formula is
//!   the circle through x and y tangent at x.
//! - `B90`: B, counting a ball only when the angle at its centre between the point and y
//!   exceeds 90°, so that a convex edge's ball, which touches two faces at right angles, is
//!   not taken for a wall.
//!
//! Inside is a parity count along the axes, as colino's `Sign::Parity` (ADR-0027),
//! rewritten here since colino keeps it private.
//!
//! Each case says, for the points it judges, whether a thin wall and a narrow gap lie
//! there; a method finds one where its thickness is below 2h. Output: one row a case and
//! method on standard output.

use std::time::Instant;

use colino::bvh::Bvh;
use colino::distance::on_triangle;
use colino::intersect::crossing;
use colino::mesh::Mesh;

type P = [f64; 3];

const H: f64 = 1.0;
const THIN: f64 = 2.0 * H;
/// The largest ball tried: a diameter of 8h.
const CAP: f64 = 4.0 * H;

fn add(a: P, b: P) -> P {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
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

// ---- Inside, by parity along the axes (ADR-0027), as in spikes/sign ----

fn exact(a: f64, b: f64, c: f64, d: f64) -> f64 {
    let (x, y) = (a * b, c * d);
    if x.to_bits() != y.to_bits() {
        return x - y;
    }
    a.mul_add(b, -x) - c.mul_add(d, -y)
}

fn height(p: P, kz: usize, t: [P; 3]) -> Option<f64> {
    let (kx, ky) = ((kz + 1) % 3, (kz + 2) % 3);
    let q = t.map(|c| [c[kx] - p[kx], c[ky] - p[ky], c[kz] - p[kz]]);
    let side = |i: usize, j: usize| {
        let (a, b) = (q[i], q[j]);
        let e = exact(a[0], b[1], a[1], b[0]);
        if e != 0.0 {
            return e.signum();
        }
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
    let area = |i: usize, j: usize| exact(q[i][0], q[j][1], q[i][1], q[j][0]);
    let (w2, w0, w1) = (area(0, 1), area(1, 2), area(2, 0));
    let det = w0 + w1 + w2;
    if det == 0.0 {
        return None;
    }
    Some((w0 * q[0][2] + w1 * q[1][2] + w2 * q[2][2]) / det)
}

struct Shape {
    mesh: Mesh,
    bvh: Bvh,
    lo: P,
    hi: P,
}

impl Shape {
    fn new(mesh: Mesh) -> Shape {
        let bvh = Bvh::new(&mesh).expect("mesh");
        let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for v in &mesh.vertices {
            for k in 0..3 {
                lo[k] = lo[k].min(v[k] - 1.0);
                hi[k] = hi[k].max(v[k] + 1.0);
            }
        }
        Shape { mesh, bvh, lo, hi }
    }

    fn inside(&self, p: P) -> bool {
        let mut votes = 0;
        for k in 0..3 {
            let (mut a, mut b) = (p, p);
            (a[k], b[k]) = (self.lo[k].min(p[k]), self.hi[k].max(p[k]));
            let (mut all, mut above) = (0, 0);
            for i in self.bvh.near_segment(a, b) {
                if let Some(z) = self.bvh.corners(i).and_then(|t| height(p, k, t)) {
                    all += 1;
                    above += u32::from(z >= 0.0);
                }
            }
            if all % 2 == 0 {
                votes += if above % 2 == 1 { 1 } else { -1 };
            }
        }
        votes > 0
    }

    fn triangle(&self, i: usize) -> [P; 3] {
        self.mesh.triangles[i].map(|v| self.mesh.vertices[v as usize])
    }
}

// ---- Methods: each gives, at a point, its thinnest wall and narrowest gap ----

#[derive(Clone, Copy, Default)]
struct Found {
    wall: Option<f64>,
    gap: Option<f64>,
}

impl Found {
    fn take(&mut self, wall: bool, d: f64) {
        let slot = if wall { &mut self.wall } else { &mut self.gap };
        *slot = Some(slot.map_or(d, |x: f64| x.min(d)));
    }
}

fn rays(s: &Shape, x: P, n: P, own: usize) -> Found {
    let mut f = Found::default();
    let reach = dot(sub(s.hi, s.lo), sub(s.hi, s.lo)).sqrt();
    for side in [1.0, -1.0] {
        let d = scale(n, side);
        let (a, b) = (add(x, scale(d, 1e-9 * reach)), add(x, scale(d, reach)));
        let mut best: Option<f64> = None;
        for i in s.bvh.near_segment(a, b) {
            if i as usize == own {
                continue;
            }
            if let Some(t) = s.bvh.corners(i).and_then(|t| crossing(a, b, t)) {
                best = Some(best.map_or(t, |b: f64| b.min(t)));
            }
        }
        if let Some(t) = best {
            let dist = t * dot(sub(b, a), sub(b, a)).sqrt() + 1e-9 * reach;
            let mid = add(x, scale(d, dist / 2.0));
            f.take(s.inside(mid), dist);
        }
    }
    f
}

fn balls(s: &Shape, x: P, n: P, filtered: bool) -> Found {
    let mut f = Found::default();
    for side in [1.0, -1.0] {
        let d = scale(n, side);
        let mut r = CAP;
        let mut touch: Option<P> = None;
        for _ in 0..64 {
            let c = add(x, scale(d, r));
            let Some((dist, i)) = s.bvh.nearest(c) else { break };
            if dist >= r * (1.0 - 1e-9) {
                break;
            }
            let y = on_triangle(c, s.bvh.corners(i).expect("triangle"));
            let along = dot(d, sub(y, x));
            if along <= 0.0 {
                break;
            }
            let next = dot(sub(y, x), sub(y, x)) / (2.0 * along);
            if next >= r {
                break;
            }
            r = next;
            touch = Some(y);
        }
        let Some(y) = touch else { continue };
        let c = add(x, scale(d, r));
        let (u, v) = (sub(x, c), sub(y, c));
        let cos = dot(u, v) / (dot(u, u).sqrt() * dot(v, v).sqrt());
        if filtered && cos >= 0.0 {
            continue;
        }
        f.take(s.inside(c), 2.0 * r);
    }
    f
}

// ---- Shapes, from textbook constructions ----

fn turn(p: P) -> P {
    let (a, b) = (0.5_f64, 0.3_f64);
    let [x, y, z] = p;
    let (y, z) = (y * a.cos() - z * a.sin(), y * a.sin() + z * a.cos());
    let (x, y) = (x * b.cos() - y * b.sin(), x * b.sin() + y * b.cos());
    [x + 0.31, y + 0.17, z - 0.23]
}

fn unturn(p: P) -> P {
    let (a, b) = (0.5_f64, 0.3_f64);
    let [x, y, z] = [p[0] - 0.31, p[1] - 0.17, p[2] + 0.23];
    let (x, y) = (x * b.cos() + y * b.sin(), -x * b.sin() + y * b.cos());
    let (y, z) = (y * a.cos() + z * a.sin(), -y * a.sin() + z * a.cos());
    [x, y, z]
}

/// An axis-aligned box, facing out, each face split in two.
fn boxed(lo: P, hi: P) -> (Vec<P>, Vec<[u32; 3]>) {
    let v = (0..8).map(|i| [0, 1, 2].map(|k| if i >> k & 1 == 1 { hi[k] } else { lo[k] })).collect();
    let t = vec![
        [0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4],
        [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5],
    ];
    (v, t)
}

fn mesh_of(parts: Vec<(Vec<P>, Vec<[u32; 3]>)>, map: impl Fn(P) -> P) -> Mesh {
    let (mut vertices, mut triangles) = (Vec::new(), Vec::new());
    for (v, t) in parts {
        let base = vertices.len() as u32;
        vertices.extend(v.into_iter().map(&map));
        triangles.extend(t.into_iter().map(|t| t.map(|i| i + base)));
    }
    Mesh::new(vertices, triangles)
}

/// A surface of revolution about z of the profile (r, z), from pole to pole, facing out.
fn revolved(profile: &[(f64, f64)], m: usize) -> Mesh {
    let mut vertices = Vec::new();
    let mut rings = Vec::new();
    for &(r, z) in profile {
        if r == 0.0 {
            rings.push(vec![vertices.len() as u32]);
            vertices.push([0.0, 0.0, z]);
        } else {
            let start = vertices.len() as u32;
            for j in 0..m {
                let a = std::f64::consts::TAU * j as f64 / m as f64;
                vertices.push([r * a.cos(), r * a.sin(), z]);
            }
            rings.push((start..start + m as u32).collect());
        }
    }
    let mut triangles = Vec::new();
    for w in rings.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        for j in 0..m {
            let k = (j + 1) % m;
            match (a.len(), b.len()) {
                (1, _) => triangles.push([a[0], b[k], b[j]]),
                (_, 1) => triangles.push([a[j], a[k], b[0]]),
                _ => {
                    triangles.push([a[j], a[k], b[k]]);
                    triangles.push([a[j], b[k], b[j]]);
                }
            }
        }
    }
    Mesh::new(vertices, triangles)
}

fn torus(big: f64, small: f64, n: usize, m: usize) -> Mesh {
    let mut vertices = Vec::new();
    for i in 0..n {
        let u = std::f64::consts::TAU * i as f64 / n as f64;
        for j in 0..m {
            let v = std::f64::consts::TAU * j as f64 / m as f64;
            let r = big + small * v.cos();
            vertices.push([r * u.cos(), r * u.sin(), small * v.sin()]);
        }
    }
    let at = |i: usize, j: usize| ((i % n) * m + j % m) as u32;
    let mut triangles = Vec::new();
    for i in 0..n {
        for j in 0..m {
            triangles.push([at(i, j), at(i + 1, j), at(i + 1, j + 1)]);
            triangles.push([at(i, j), at(i + 1, j + 1), at(i, j + 1)]);
        }
    }
    Mesh::new(vertices, triangles)
}

fn flipped(m: &Mesh, every: usize) -> Mesh {
    let t = m.triangles.iter().enumerate().map(|(i, t)| if i % every == 0 { [t[0], t[2], t[1]] } else { *t }).collect();
    Mesh::new(m.vertices.clone(), t)
}

/// What a case says at a point: whether a thin wall and a narrow gap lie there, or
/// nothing where it does not judge.
type Truth = Box<dyn Fn(P) -> Option<(bool, bool)>>;

struct Case {
    name: String,
    mesh: Mesh,
    truth: Truth,
    /// The thickness or width the case was built with, where it has one.
    built: Option<f64>,
}

fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    for t in [1.0, 1.5, 2.5, 3.0] {
        for broken in [false, true] {
            let mut mesh = mesh_of(vec![boxed([-10.0, -10.0, -t / 2.0], [10.0, 10.0, t / 2.0])], turn);
            if broken {
                mesh = flipped(&mesh, 3);
            }
            out.push(Case {
                name: format!("slab_{t}{}", if broken { "_turned" } else { "" }),
                mesh,
                truth: Box::new(move |p| {
                    let q = unturn(p);
                    let face = (q[2].abs() - t / 2.0).abs() < 1e-6;
                    (face && q[0].abs() < 8.0 && q[1].abs() < 8.0).then_some((t < THIN, false))
                }),
                built: Some(t),
            });
        }
    }
    for g in [0.5, 1.5, 2.5, 3.0] {
        for broken in [false, true] {
            let parts = vec![
                boxed([-10.0 - g / 2.0, -5.0, -5.0], [-g / 2.0, 5.0, 5.0]),
                boxed([g / 2.0, -5.0, -5.0], [10.0 + g / 2.0, 5.0, 5.0]),
            ];
            let mut mesh = mesh_of(parts, turn);
            if broken {
                mesh = flipped(&mesh, 3);
            }
            out.push(Case {
                name: format!("gap_{g}{}", if broken { "_turned" } else { "" }),
                mesh,
                truth: Box::new(move |p| {
                    let q = unturn(p);
                    let facing = (q[0].abs() - g / 2.0).abs() < 1e-6;
                    let away = q[1].abs() < 3.0 && q[2].abs() < 3.0;
                    if facing && away {
                        return Some((false, g < THIN));
                    }
                    // Faces far from every edge and from the gap.
                    let far = (q[0].abs() - (10.0 + g / 2.0)).abs() < 1e-6 && away;
                    far.then_some((false, false))
                }),
                built: Some(g),
            });
        }
    }
    // Two lobes of radius 5 on a neck of radius 0.7, a flat shoulder between.
    let mut profile = vec![(0.0, -12.0)];
    for i in 1..=20 {
        let a = std::f64::consts::PI * f64::from(i) / 24.0;
        profile.push((5.0 * a.sin(), -7.0 - 5.0 * a.cos()));
    }
    profile.push((0.7, -3.0));
    profile.extend((-2..=2).map(|z| (0.7, f64::from(z))));
    profile.push((0.7, 3.0));
    for i in (1..=20).rev() {
        let a = std::f64::consts::PI * f64::from(i) / 24.0;
        profile.push((5.0 * a.sin(), 7.0 + 5.0 * a.cos()));
    }
    profile.push((0.0, 12.0));
    out.push(Case {
        name: "dumbbell".into(),
        mesh: revolved(&profile, 48),
        truth: Box::new(|p| {
            let r = (p[0] * p[0] + p[1] * p[1]).sqrt();
            if p[2].abs() < 2.0 && (r - 0.7).abs() < 0.1 {
                return Some((true, false));
            }
            (p[2].abs() > 7.0 && r > 3.0).then_some((false, false))
        }),
        built: Some(1.4),
    });
    // A cone of half-angle atan(1/2), its apex at z = 0, its base at z = 10.
    let mut cone: Vec<(f64, f64)> = vec![(0.0, 0.0)];
    cone.extend((1..=20).map(|i| (f64::from(i) * 0.25, f64::from(i) * 0.5)));
    cone.push((0.0, 10.0));
    out.push(Case {
        name: "cone".into(),
        mesh: revolved(&cone, 48),
        truth: Box::new(|p| {
            let side = p[2] < 9.9;
            if side && p[2] < 1.0 {
                return Some((true, false));
            }
            (side && p[2] > 6.0 && p[2] < 9.0).then_some((false, false))
        }),
        built: None,
    });
    // A wedge plate, 20 long, its thickness growing from 0.5 to 4.5 along x: its top face
    // slants, so the rays from it are not across the plate.
    let wedge_v: Vec<P> = (0..8)
        .map(|i| {
            let x = if i & 1 == 1 { 10.0 } else { -10.0 };
            let y = if i & 2 == 2 { 6.0 } else { -6.0 };
            let top = 0.5 + 4.0 * (x + 10.0) / 20.0;
            [x, y, if i & 4 == 4 { top } else { 0.0 }]
        })
        .collect();
    let (_, wedge_t) = boxed([0.0; 3], [1.0; 3]);
    out.push(Case {
        name: "wedge".into(),
        mesh: mesh_of(vec![(wedge_v, wedge_t)], turn),
        truth: Box::new(|p| {
            let q = unturn(p);
            let top = 0.5 + 4.0 * (q[0] + 10.0) / 20.0;
            let face = q[2].abs() < 1e-6 || (q[2] - top).abs() < 1e-6;
            // The thickness across the plate, top · cos of the slope, is 2 at x ≈ −2.5.
            let thickness = top * (1.0 / (1.0_f64 + 0.04).sqrt());
            let judged = face && q[1].abs() < 4.0 && (thickness < 1.6 || thickness > 2.4);
            judged.then_some((thickness < THIN, false))
        }),
        built: None,
    });
    // A sphere of radius 6 above a plate, 0.3 between them at the nearest: the gap is narrow
    // only round the nearest point.
    let mut ball = vec![(0.0, 0.3)];
    ball.extend((1..60).map(|i| {
        let a = std::f64::consts::PI * f64::from(i) / 60.0;
        (6.0 * a.sin(), 6.3 - 6.0 * a.cos())
    }));
    ball.push((0.0, 12.3));
    let ball = revolved(&ball, 96);
    let plate = boxed([-10.0, -10.0, -4.0], [10.0, 10.0, 0.0]);
    out.push(Case {
        name: "ball_over_plate".into(),
        mesh: mesh_of(vec![(ball.vertices.clone(), ball.triangles.clone()), plate], |p| p),
        truth: Box::new(|p| {
            let r = (p[0] * p[0] + p[1] * p[1]).sqrt();
            // The gap below the ball at radius r: the ball's underside above the plate.
            let under = if r < 6.0 { 6.3 - (36.0 - r * r).sqrt() } else { f64::INFINITY };
            let on_plate = p[2].abs() < 1e-6 && r < 9.0;
            let on_ball = (p[2] - under).abs() < 0.05 && r < 5.0;
            if !(on_plate || on_ball) || (under - THIN).abs() < 0.4 {
                return None;
            }
            Some((false, under < THIN))
        }),
        built: Some(0.3),
    });
    let mut sphere = vec![(0.0, -8.0)];
    sphere.extend((1..40).map(|i| {
        let a = std::f64::consts::PI * f64::from(i) / 40.0;
        (8.0 * a.sin(), -8.0 * a.cos())
    }));
    sphere.push((0.0, 8.0));
    out.push(Case { name: "sphere".into(), mesh: revolved(&sphere, 64), truth: Box::new(|_| Some((false, false))), built: None });
    out.push(Case { name: "torus_thin".into(), mesh: torus(8.0, 0.7, 96, 12), truth: Box::new(|_| Some((true, false))), built: Some(1.4) });
    out
}

/// Points about `step` apart on a triangle: the centroids of a k-fold split of it.
fn samples(t: [P; 3], step: f64) -> Vec<P> {
    let longest = [(0, 1), (1, 2), (2, 0)].iter().map(|(i, j)| dot(sub(t[*j], t[*i]), sub(t[*j], t[*i])).sqrt()).fold(0.0, f64::max);
    let k = (longest / step).ceil().max(1.0) as usize;
    let (u, v) = (sub(t[1], t[0]), sub(t[2], t[0]));
    let mut out = Vec::new();
    for i in 0..k {
        for j in 0..k - i {
            let (a, b) = ((i as f64 + 1.0 / 3.0) / k as f64, (j as f64 + 1.0 / 3.0) / k as f64);
            out.push(add(t[0], add(scale(u, a), scale(v, b))));
        }
    }
    out
}

fn main() {
    println!("case\tmethod\tjudged\tthin\tfound\tmissed\tfalse\twall_median\tms");
    let only = std::env::var("ONLY").ok();
    for case in cases() {
        if only.as_deref().is_some_and(|o| o != case.name) {
            continue;
        }
        let s = Shape::new(case.mesh);
        let mut points: Vec<(P, P, usize, (bool, bool))> = Vec::new();
        for i in 0..s.mesh.triangles.len() {
            let t = s.triangle(i);
            let n = cross3(sub(t[1], t[0]), sub(t[2], t[0]));
            if dot(n, n) == 0.0 {
                continue;
            }
            let n = unit(n);
            for p in samples(t, H / 2.0) {
                if let Some(truth) = (case.truth)(p) {
                    points.push((p, n, i, truth));
                }
            }
        }
        let thin = points.iter().filter(|(_, _, _, (w, g))| *w || *g).count();
        for method in ["A", "B", "B90"] {
            let t0 = Instant::now();
            let found: Vec<Found> = par_map(points.len(), |k| {
                let (p, n, own, _) = points[k];
                match method {
                    "A" => rays(&s, p, n, own),
                    "B" => balls(&s, p, n, false),
                    _ => balls(&s, p, n, true),
                }
            });
            let ms = t0.elapsed().as_secs_f64() * 1e3;
            let (mut hit, mut missed, mut wrong) = (0, 0, 0);
            let mut walls = Vec::new();
            for ((pt, _, _, (w, g)), f) in points.iter().zip(&found) {
                let fw = f.wall.is_some_and(|d| d < THIN);
                let fg = f.gap.is_some_and(|d| d < THIN);
                if *w || *g {
                    if (*w && fw) || (*g && fg) { hit += 1 } else { missed += 1 }
                }
                wrong += usize::from(fw && !*w) + usize::from(fg && !*g);
                if std::env::var("SHOW").ok().as_deref() == Some(method) && ((fw && !*w) || (fg && !*g)) {
                    eprintln!("false {method} at {:?} wall {:?} gap {:?}", unturn(*pt), f.wall, f.gap);
                }
                if let Some(d) = if case.name.starts_with("gap") { f.gap } else { f.wall } {
                    walls.push(d);
                }
            }
            walls.sort_by(f64::total_cmp);
            let median = walls.get(walls.len() / 2).map_or("-".to_owned(), |d| format!("{d:.3}"));
            let built = case.built.map_or(String::new(), |b| format!(" (built {b})"));
            println!("{}\t{method}\t{}\t{thin}\t{hit}\t{missed}\t{wrong}\t{median}{built}\t{ms:.1}", case.name, points.len());
        }
    }
}
