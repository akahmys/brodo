//! Reference tier (TESTING.md §1) for segments against triangles (ADR-0030): on integer
//! coordinates the crossing test agrees with the same test in exact `i128` arithmetic,
//! ties on edges and corners included; where products round equal it still decides as the
//! exact arithmetic does; a closed mesh lets no segment through at a corner or a shared
//! edge; the nearest point of a segment agrees with a dense search; the tree's search
//! keeps every triangle a segment crosses.

mod common;

use colino::bvh::Bvh;
use colino::distance::{Point, on_triangle, squared};
use colino::intersect::{crossing, nearest};
use proptest::prelude::*;

/// How far a crossing's fraction may be from the exact one: a few roundings of a division.
const FRACTION_TOLERANCE: f64 = 1e-12;
/// How far a nearest distance may be above a dense search's, relative to the shapes' size.
const DENSE_TOLERANCE: f64 = 1e-9;

/// The test in exact arithmetic on integer coordinates, for a segment along axis `k`
/// from `a`, of signed length `len`: the crossing's fraction as a numerator and positive
/// denominator, or nothing.
fn exact(a: [i64; 3], k: usize, len: i64, t: [[i64; 3]; 3]) -> Option<(i128, i128)> {
    let (kx, ky) = ((k + 1) % 3, (k + 2) % 3);
    let rel = t.map(|p| [0, 1, 2].map(|i| i128::from(p[i] - a[i])));
    let [pa, pb, pc] = rel;
    let edge = |p: [i128; 3], q: [i128; 3]| p[kx] * q[ky] - p[ky] * q[kx];
    let (u, v, w) = (edge(pc, pb), edge(pa, pc), edge(pb, pa));
    if (u < 0 || v < 0 || w < 0) && (u > 0 || v > 0 || w > 0) {
        return None;
    }
    let det = u + v + w;
    if det == 0 {
        return None;
    }
    // The fraction is along / (det · len).
    let along = u * pa[k] + v * pb[k] + w * pc[k];
    let (mut num, mut den) = (along, det * i128::from(len));
    if den < 0 {
        (num, den) = (-num, -den);
    }
    (0 <= num && num <= den).then_some((num, den))
}

fn point(p: [i64; 3]) -> Point {
    p.map(|c| c as f64)
}

fn small() -> impl Strategy<Value = [i64; 3]> {
    [-4_i64..=4, -4_i64..=4, -4_i64..=4]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4096))]

    /// Small coordinates: products are exact, so ties on edges and corners are frequent and
    /// exact, and every decision must be the exact one.
    #[test]
    fn small_integer_cases_decide_as_exact_arithmetic(
        a in small(), k in 0_usize..3, len in prop_oneof![-6_i64..=-1, 1_i64..=6],
        t in [small(), small(), small()],
    ) {
        let mut b = a;
        b[k] += len;
        let got = crossing(point(a), point(b), t.map(point));
        match exact(a, k, len, t) {
            None => prop_assert_eq!(got, None),
            Some((num, den)) => {
                let s = got.ok_or_else(|| TestCaseError::fail("missed an exact crossing"))?;
                let want = num as f64 / den as f64;
                prop_assert!((s - want).abs() <= FRACTION_TOLERANCE, "{s} against {want}");
            }
        }
    }

    /// Large coordinates, the segment passing within a few units of an edge: the edge's two
    /// products, near 2⁸³, round equal though they differ by `d`, and only the tie rule
    /// decides whether the segment hits.
    #[test]
    fn rounded_ties_decide_as_exact_arithmetic(
        b in (1_i64 << 40..1_i64 << 41, 1_i64 << 40..1_i64 << 41)
            .prop_filter("coprime", |(x, y)| gcd(*x, *y) == 1),
        d in -3_i64..=3, k in 0_usize..3, far in 1_i64..1 << 20,
    ) {
        let (bx, by) = b;
        // C1 with det(C1, B) = 1; then C = d C1 − 2B has det(C, B) = d, and lies across the
        // segment's axis from B.
        let (cx1, cy1) = bezout(bx, by);
        let (cx, cy) = (d * cx1 - 2 * bx, d * cy1 - 2 * by);
        let (kx, ky) = ((k + 1) % 3, (k + 2) % 3);
        let mut pts = [[5_i64; 3]; 3];
        (pts[1][kx], pts[1][ky]) = (bx, by);
        (pts[2][kx], pts[2][ky]) = (cx, cy);
        (pts[0][kx], pts[0][ky]) = (-far, far);
        let a = [0_i64; 3];
        let mut end = a;
        end[k] = 10;
        let got = crossing(point(a), point(end), pts.map(point)).is_some();
        prop_assert_eq!(got, exact(a, k, 10, pts).is_some());
    }
}

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 { a.abs() } else { gcd(b, a % b) }
}

/// `(x, y)` with `x·by − y·bx = 1`, for coprime `bx` and `by`.
fn bezout(bx: i64, by: i64) -> (i64, i64) {
    // Extended Euclid on (by, bx): s·by + t·bx = 1, so x = s and y = −t.
    let (mut r0, mut r1, mut s0, mut s1, mut t0, mut t1) = (by, bx, 1_i128, 0_i128, 0_i128, 1_i128);
    while r1 != 0 {
        let q = r0 / r1;
        (r0, r1) = (r1, r0 - q * r1);
        (s0, s1) = (s1, s0 - i128::from(q) * s1);
        (t0, t1) = (t1, t0 - i128::from(q) * t1);
    }
    let sign = if r0 < 0 { -1 } else { 1 };
    (i64::try_from(sign * s0).unwrap(), i64::try_from(-sign * t0).unwrap())
}

/// Points on a closed sphere far from the origin: segments along each axis through every
/// corner and through points of every shared edge each cross at least one triangle.
#[test]
fn a_closed_mesh_lets_no_segment_through_a_corner_or_an_edge() {
    let centre = [1000.25, -2000.5, 3000.125];
    let m = common::sphere(3, 7.3, centre);
    let corners: Vec<[Point; 3]> =
        m.triangles.iter().map(|t| t.map(|i| m.vertices[i as usize])).collect();
    let corner_count = m.vertices.len();
    let mut points: Vec<Point> = m.vertices.clone();
    for t in &m.triangles {
        for (i, j) in [(0, 1), (1, 2), (2, 0)] {
            let (p, q) = (m.vertices[t[i] as usize], m.vertices[t[j] as usize]);
            for f in [0.25, 0.5, 0.75] {
                points.push([0, 1, 2].map(|c| p[c] + f * (q[c] - p[c])));
            }
        }
    }
    let mut checked = [0, 0];
    for (i, p) in points.iter().enumerate() {
        let n = [0, 1, 2].map(|c| (p[c] - centre[c]) / 7.3);
        for k in 0..3 {
            // Skip grazing segments, which may rightly touch the surface or not.
            if n[k].abs() < 0.3 {
                continue;
            }
            let (mut a, mut b) = (*p, *p);
            a[k] -= 1.0;
            b[k] += 1.0;
            let hits = corners.iter().filter(|t| crossing(a, b, **t).is_some()).count();
            assert!(hits >= 1, "through {p:?} along {k}: no triangle hit");
            checked[usize::from(i >= corner_count)] += 1;
        }
    }
    // Reached: segments through corners and through edges, each many times.
    assert!(checked[0] > 500 && checked[1] > 1000, "{checked:?} checked");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// The nearest point of a segment to a triangle: no dense pair of points is nearer, and
    /// the point given is as near as it says.
    #[test]
    fn the_nearest_point_of_a_segment_is_no_further_than_a_dense_search(
        a in [-5.0_f64..5.0, -5.0..5.0, -5.0..5.0], b in [-5.0_f64..5.0, -5.0..5.0, -5.0..5.0],
        t in [[-5.0_f64..5.0, -5.0..5.0, -5.0..5.0], [-5.0..5.0, -5.0..5.0, -5.0..5.0],
              [-5.0..5.0, -5.0..5.0, -5.0..5.0]],
    ) {
        let (s, d) = nearest(a, b, t);
        prop_assert!((0.0..=1.0).contains(&s));
        let at = |s: f64| [0, 1, 2].map(|c| a[c] + s * (b[c] - a[c]));
        let q = at(s);
        prop_assert!((squared(q, on_triangle(q, t)) - d).abs() <= DENSE_TOLERANCE * 100.0);
        let dense = (0..=400).map(|i| {
            let p = at(f64::from(i) / 400.0);
            squared(p, on_triangle(p, t))
        });
        let best = dense.fold(f64::INFINITY, f64::min);
        prop_assert!(d <= best + DENSE_TOLERANCE * 100.0, "{d} above {best}");
    }
}

#[test]
fn the_trees_search_keeps_every_triangle_a_segment_crosses() {
    let m = common::torus(20.0, 7.0, 24);
    let bvh = Bvh::new(&m).unwrap();
    let corners: Vec<[Point; 3]> =
        m.triangles.iter().map(|t| t.map(|i| m.vertices[i as usize])).collect();
    let mut crossed = 0;
    for x in -30..30 {
        for y in -30..30 {
            let (a, b) = ([f64::from(x), f64::from(y), -10.0], [f64::from(x), f64::from(y), 10.0]);
            let near = bvh.near_segment(a, b);
            assert!(near.windows(2).all(|w| w[0] < w[1]), "ascending");
            for (i, t) in (0_u32..).zip(&corners) {
                if crossing(a, b, *t).is_some() {
                    assert!(near.contains(&i), "triangle {i} crossed but not found");
                    crossed += 1;
                }
                let lo =
                    t.iter().fold([f64::INFINITY; 3], |m, p| [0, 1, 2].map(|c| m[c].min(p[c])));
                let hi =
                    t.iter().fold([f64::NEG_INFINITY; 3], |m, p| [0, 1, 2].map(|c| m[c].max(p[c])));
                let apart = [0, 1, 2]
                    .iter()
                    .any(|&c| lo[c] > a[c].max(b[c]) + 1e-9 || hi[c] < a[c].min(b[c]) - 1e-9);
                assert!(!(apart && near.contains(&i)), "triangle {i} found though apart");
            }
        }
    }
    assert!(crossed > 100, "{crossed} crossings");
}

/// A triangle one ulp beyond the segment's end, which rounding lets the test hit: the
/// tree's search, widened by that rounding, still finds it (ADR-0030).
#[test]
fn a_crossing_rounding_puts_past_the_end_is_still_found() {
    let z = 1.0 + f64::EPSILON;
    let t = [
        [1.643_910_620_122_558_8, 0.162_921_552_616_507_54, z],
        [2.445_275_961_559_378_4, 1.620_831_938_547_384_8, z],
        [-1.862_246_729_429_496, -1.134_840_155_711_500_2, z],
    ];
    let (a, b) = ([0.0; 3], [0.0, 0.0, 1.0]);
    assert!(crossing(a, b, t).is_some(), "rounding no longer reaches past the end");
    let m = colino::mesh::Mesh {
        vertices: t.to_vec(),
        triangles: vec![[0, 1, 2]],
        ..colino::mesh::Mesh::default()
    };
    assert_eq!(Bvh::new(&m).unwrap().near_segment(a, b), vec![0]);
}

/// The same past the end, under a triangle thousands of units wide: the widening follows
/// the largest coordinate the search sees, so it still finds it.
#[test]
fn a_crossing_past_the_end_under_a_wide_triangle_is_still_found() {
    let z = 1.0 + f64::EPSILON;
    let t = [
        [-2_513.774_746_881_219, 2_759.691_683_418_149, z],
        [2_760.577_330_185_404, -824.801_710_038_715_7, z],
        [763.210_924_871_714_5, -1_708.689_310_165_436_2, z],
    ];
    let (a, b) = ([0.0; 3], [0.0, 0.0, 1.0]);
    assert!(crossing(a, b, t).is_some(), "rounding no longer reaches past the end");
    let m = colino::mesh::Mesh {
        vertices: t.to_vec(),
        triangles: vec![[0, 1, 2]],
        ..colino::mesh::Mesh::default()
    };
    assert_eq!(Bvh::new(&m).unwrap().near_segment(a, b), vec![0]);
}

#[test]
fn a_tree_of_no_triangle_is_empty() {
    let empty = Bvh::new(&colino::mesh::Mesh::default()).unwrap();
    assert!(empty.is_empty());
    assert_eq!(empty.len(), 0);
    let one = Bvh::new(&common::sphere(0, 1.0, [0.0; 3])).unwrap();
    assert!(!one.is_empty() && one.len() == 8);
}
