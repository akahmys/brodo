//! The sign that does not depend on orientation (ARCHITECTURE.md §5.1, ADR-0027): a point
//! is inside where most of the three lines through it along the axes cross the mesh an
//! odd number of times, by Nooruddin and Turk's parity count (IEEE TVCG 9(2), 2003, §3.1).
//! A line crossing an odd number of times passes through a hole and does not vote; a tie
//! is outside.
//!
//! Each line is tested against the triangles whose boxes meet it, as Woop, Benthin and
//! Wald's test does after its shear (JCGT 2(1), 2013), which along an axis is no shear: the
//! triangle is moved so that the line is the origin of the plane across it, and each edge
//! is a 2D edge function of its corners. A line exactly on an edge or a corner, an edge
//! function of exactly 0, takes the sign the function has with the line moved by (ε, ε²)
//! across itself, so that it is inside exactly the triangles the moved line is inside: an
//! edge's two triangles count once between them, a corner's fan once, and a line grazing a
//! fold none or twice. A triangle seen edge on is not crossed.

use crate::bvh::Bvh;
use crate::distance::Point;
use crate::intersect::edge;

/// The sign at the plane's origin of the edge from `a` to `b`: of (b − a) × (o − a), the
/// function exactly 0 decided by the line moved by (ε, ε²), whose first-order term is
/// −(b_y − a_y) ε and whose second is (b_x − a_x) ε².
fn side(a: [f64; 2], b: [f64; 2]) -> i8 {
    let sign = |x: f64| {
        if x > 0.0 {
            1
        } else if x < 0.0 {
            -1
        } else {
            0
        }
    };
    let e = edge(a[0], b[1], a[1], b[0]);
    if e != 0.0 {
        return sign(e);
    }
    let dy = b[1] - a[1];
    if dy != 0.0 {
        return -sign(dy);
    }
    sign(b[0] - a[0])
}

/// Where the line through `p` along axis `k` crosses triangle `t`, as its height along the
/// axis above `p`; nothing where it does not.
fn height(p: Point, k: usize, t: [Point; 3]) -> Option<f64> {
    let (kx, ky) = ((k + 1) % 3, (k + 2) % 3);
    let q = t.map(|c| [c[kx] - p[kx], c[ky] - p[ky], c[k] - p[k]]);
    let flat = q.map(|c| [c[0], c[1]]);
    let [a, b, c] = flat;
    let s = [side(a, b), side(b, c), side(c, a)];
    if !(s[0] == s[1] && s[1] == s[2]) || s[0] == 0 {
        return None;
    }
    // Each corner's weight, the signed area its opposite edge spans with the origin.
    let area = |u: [f64; 2], v: [f64; 2]| edge(u[0], v[1], u[1], v[0]);
    let w = [area(b, c), area(c, a), area(a, b)];
    let det = w[0] + w[1] + w[2];
    if det == 0.0 {
        return None;
    }
    Some((w[0] * q[0][2] + w[1] * q[1][2] + w[2] * q[2][2]) / det)
}

/// The mesh's box grown by 1 on every side, or nothing for a mesh with no triangle.
fn reach(bvh: &Bvh) -> Option<(Point, Point)> {
    let (lo, hi) = bvh.bounds()?;
    Some((lo.map(|c| c - 1.0), hi.map(|c| c + 1.0)))
}

/// Whether `p` is inside the mesh `bvh` holds, by the parity count along the three axes.
pub(crate) fn inside(bvh: &Bvh, p: Point) -> bool {
    let Some((lo, hi)) = reach(bvh) else { return false };
    let mut votes = 0_i32;
    for k in 0..3 {
        let (mut a, mut b) = (p, p);
        (a[k], b[k]) = (lo[k].min(p[k]), hi[k].max(p[k]));
        let (mut all, mut above) = (0_u32, 0_u32);
        for i in bvh.near_segment(a, b) {
            let Some(t) = bvh.corners(i) else { continue };
            if let Some(z) = height(p, k, t) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::Mesh;

    /// Lines through a shared edge, a shared corner and a fold, counted by the triangles
    /// that hold them, every way each triangle can be turned.
    #[test]
    fn a_line_on_an_edge_or_a_corner_counts_once_and_a_fold_none_or_twice() {
        // Two triangles in the plane z = 1 sharing the edge from (0, 0) to (1, 1).
        let (o, x, d, y) = ([0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [1.0, 1.0, 1.0], [0.0, 1.0, 1.0]);
        let on_edge = [0.5, 0.5, 0.0];
        for (first, second) in
            [([o, x, d], [o, d, y]), ([o, d, x], [o, d, y]), ([x, o, d], [y, o, d])]
        {
            let hits = [first, second].iter().filter(|t| height(on_edge, 2, **t).is_some()).count();
            assert_eq!(hits, 1, "{first:?} {second:?}");
        }
        // A fan of four about (0, 0) at z = 1, the line through its centre.
        let ring = [[1.0, 0.0, 1.0], [0.0, 1.0, 1.0], [-1.0, 0.0, 1.0], [0.0, -1.0, 1.0]];
        let fan: Vec<[Point; 3]> =
            (0..4).map(|i| [[0.0, 0.0, 1.0], ring[i], ring[(i + 1) % 4]]).collect();
        let centre = [0.0, 0.0, 0.0];
        for turned in 0..16 {
            let hits = fan
                .iter()
                .enumerate()
                .filter(|(i, t)| {
                    let t = if turned >> i & 1 == 1 { [t[0], t[2], t[1]] } else { **t };
                    height(centre, 2, t).is_some()
                })
                .count();
            assert_eq!(hits, 1, "turned {turned:04b}");
        }
        // A roof folded along x = 0.5 seen from below: both faces lie on one side of the fold
        // in the plane, so the line along it crosses neither or both.
        let ridge = ([0.5, 0.0, 2.0], [0.5, 1.0, 2.0]);
        let left = [ridge.0, ridge.1, [0.0, 0.5, 1.0]];
        let right = [ridge.0, ridge.1, [0.1, 0.5, 3.0]];
        let hits =
            [left, right].iter().filter(|t| height([0.5, 0.5, 0.0], 2, **t).is_some()).count();
        assert!(hits == 0 || hits == 2, "{hits}");
    }

    /// A cube's axis lines through its faces' split edges, and through its corners' lines,
    /// sign it as its centre is, whichever faces are turned.
    #[test]
    fn a_cube_is_inside_along_its_split_edges_whatever_its_faces() {
        let corner = |i: u32| [0, 1, 2].map(|k| if i >> k & 1 == 1 { 1.0 } else { -1.0 });
        let faces: [[u32; 3]; 12] = [
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
        for turned in [0_u32, 0b1100, 0xfff, 0b1010_0101_0011] {
            let triangles = faces
                .iter()
                .enumerate()
                .map(|(i, t)| if turned >> i & 1 == 1 { [t[0], t[2], t[1]] } else { *t })
                .collect();
            let m = Mesh::new((0..8).map(corner).collect(), triangles);
            let bvh = Bvh::new(&m).unwrap();
            for p in [[0.0, 0.0, 0.0], [0.25, 0.25, 0.25], [-0.5, -0.5, 0.3], [0.9, -0.9, 0.9]] {
                assert!(inside(&bvh, p), "{turned:b} {p:?}");
            }
            for p in [[1.5, 0.0, 0.0], [1.5, 1.5, 1.5], [0.5, 0.5, -1.25]] {
                assert!(!inside(&bvh, p), "{turned:b} {p:?}");
            }
        }
    }
}
