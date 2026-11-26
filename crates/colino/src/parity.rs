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

use std::collections::BTreeMap;

use brodo::tree::Coord;

use crate::bvh::Bvh;
use crate::distance::{Point, has_area};
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

/// A triangle a line along an axis crosses: each corner's weight, the signed area its
/// opposite edge spans with the line in the plane across the axis, each corner's place
/// along the axis, and the weights' sum.
#[derive(Debug, Clone, Copy)]
struct Crossed {
    w: [f64; 3],
    along: [f64; 3],
    det: f64,
}

impl Crossed {
    /// The crossing's height along the axis above the line's point at `at` on it.
    fn above(&self, at: f64) -> f64 {
        let [a, b, c] = self.along.map(|x| x - at);
        (self.w[0] * a + self.w[1] * b + self.w[2] * c) / self.det
    }
}

/// Whether the line through `p` along axis `k` crosses triangle `t`, by `p`'s coordinates
/// across the axis alone.
fn crossed(p: Point, k: usize, t: [Point; 3]) -> Option<Crossed> {
    // Moved to the line, a triangle's corners on a line may round off it: it is refused by
    // its own corners first.
    if !has_area(t) {
        return None;
    }
    let (kx, ky) = ((k + 1) % 3, (k + 2) % 3);
    let [a, b, c] = t.map(|c| [c[kx] - p[kx], c[ky] - p[ky]]);
    let s = [side(a, b), side(b, c), side(c, a)];
    if !(s[0] == s[1] && s[1] == s[2]) || s[0] == 0 {
        return None;
    }
    let area = |u: [f64; 2], v: [f64; 2]| edge(u[0], v[1], u[1], v[0]);
    let w = [area(b, c), area(c, a), area(a, b)];
    let det = w[0] + w[1] + w[2];
    if det == 0.0 {
        return None;
    }
    Some(Crossed { w, along: t.map(|c| c[k]), det })
}

/// Where the line through `p` along axis `k` crosses triangle `t`, as its height along the
/// axis above `p`; nothing where it does not.
#[cfg(test)]
fn height(p: Point, k: usize, t: [Point; 3]) -> Option<f64> {
    crossed(p, k, t).map(|c| c.above(p[k]))
}

/// The triangles one line along an axis crosses, found once for every point on it.
#[derive(Debug)]
struct Line {
    crossed: Vec<Crossed>,
}

impl Line {
    /// The line through `p` along axis `k`, run across the mesh's box, `p` within it: the
    /// segment only finds the triangles whose boxes it meets, with the query's margin, and
    /// each is then tested whole. Which triangles it crosses does not depend on `p[k]`:
    /// one crossed holds `p`'s other two coordinates within its box exactly, its corners
    /// moved to the line keeping their signs, and along the axis lies within the mesh's box,
    /// which the segment spans wherever `p` is on the line.
    fn new(bvh: &Bvh, p: Point, k: usize) -> Line {
        let Some((lo, hi)) = bvh.bounds() else { return Line { crossed: Vec::new() } };
        let (mut a, mut b) = (p, p);
        (a[k], b[k]) = (lo[k].min(p[k]), hi[k].max(p[k]));
        let crossed = bvh
            .near_segment(a, b)
            .into_iter()
            .filter_map(|i| crossed(p, k, bvh.corners(i)?))
            .collect();
        Line { crossed }
    }

    /// The line's vote at its point at `at`: none where it crosses the mesh an odd number of
    /// times, else 1 where it crosses an odd number of times above the point, −1 otherwise.
    fn vote(&self, at: f64) -> i32 {
        if self.crossed.len() % 2 == 1 {
            return 0;
        }
        let above = self.crossed.iter().filter(|c| c.above(at) >= 0.0).count();
        if above % 2 == 1 { 1 } else { -1 }
    }
}

/// Whether `p` is inside the mesh `bvh` holds, by the parity count along the three axes.
pub(crate) fn inside(bvh: &Bvh, p: Point) -> bool {
    (0..3).map(|k| Line::new(bvh, p, k).vote(p[k])).sum::<i32>() > 0
}

/// Lines voxels share: the eight voxels along each of a leaf's lines find its crossings
/// once (issue 23). Each line is kept by its voxels' coordinates across its axis.
pub(crate) struct Lines<'a> {
    bvh: &'a Bvh,
    found: [BTreeMap<[i32; 2], Line>; 3],
}

impl<'a> Lines<'a> {
    pub(crate) fn new(bvh: &'a Bvh) -> Lines<'a> {
        Lines { bvh, found: Default::default() }
    }

    /// Whether voxel `v`, its point at `p`, is inside, as `inside` finds it: `p`'s
    /// coordinates across each axis are those of every voxel sharing `v`'s across it, each
    /// a function of its voxel coordinate alone, as a lattice's are.
    pub(crate) fn inside(&mut self, v: Coord, p: Point) -> bool {
        let (bvh, mut votes) = (self.bvh, 0);
        for (k, found) in self.found.iter_mut().enumerate() {
            let across = [v[(k + 1) % 3], v[(k + 2) % 3]];
            let line = found.entry(across).or_insert_with(|| Line::new(bvh, p, k));
            votes += line.vote(p[k]);
        }
        votes > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::Mesh;

    /// An edge function of exactly 0 takes the sign the line moved by (ε, ε²) gives: the
    /// first-order term −(b_y − a_y) ε where it is not 0, else the second (b_x − a_x) ε²;
    /// an edge of no length, both 0, is 0.
    #[test]
    fn an_edge_through_the_line_takes_the_moved_lines_sign() {
        assert_eq!(side([0.0, -1.0], [0.0, 1.0]), -1);
        assert_eq!(side([0.0, 1.0], [0.0, -1.0]), 1);
        assert_eq!(side([-1.0, 0.0], [1.0, 0.0]), 1);
        assert_eq!(side([1.0, 0.0], [-1.0, 0.0]), -1);
        assert_eq!(side([2.0, 2.0], [2.0, 2.0]), 0);
        // Off the line, the function's own sign.
        assert_eq!(side([1.0, 0.0], [1.0, 1.0]), 1);
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(4096))]

        /// Whatever triangle an axis line through a point near it crosses has area, its
        /// edges' cross product not zero, the third corner on the line of the others but for
        /// rounding, as `crossing` holds a segment (`intersect_reference.rs`).
        #[test]
        fn whatever_an_axis_line_crosses_has_area(
            a in proptest::array::uniform3(-1.0..1.0_f64),
            b in proptest::array::uniform3(-1.0..1.0_f64),
            t in -1.0..2.0_f64,
            s in 0.0..1.0_f64,
            k in 0..3_usize,
        ) {
            let c = [0, 1, 2].map(|i| a[i] + t * (b[i] - a[i]));
            let p = [0, 1, 2].map(|i| a[i] + s * (b[i] - a[i]));
            if height(p, k, [a, b, c]).is_some() {
                proptest::prop_assert!(has_area([a, b, c]), "{:?} crossed along {}", [a, b, c], k);
            }
        }
    }

    /// Where the line through `p` along axis `k` crosses triangle `t`, as its height above
    /// `p`, the triangle moved to `p` whole and its height weighed there, a point at a time.
    fn moved_to(p: Point, k: usize, t: [Point; 3]) -> Option<f64> {
        let c = crossed(p, k, t)?;
        let q = t.map(|c| c[k] - p[k]);
        Some((c.w[0] * q[0] + c.w[1] * q[1] + c.w[2] * q[2]) / c.det)
    }

    /// The parity count by every triangle, one point at a time, as ARCHITECTURE.md §5.1
    /// defines the sign.
    fn scanned(m: &[[Point; 3]], p: Point) -> bool {
        let mut votes = 0;
        for k in 0..3 {
            let z: Vec<f64> = m.iter().filter_map(|t| moved_to(p, k, *t)).collect();
            if z.len().is_multiple_of(2) {
                votes += if z.iter().filter(|z| **z >= 0.0).count() % 2 == 1 { 1 } else { -1 };
            }
        }
        votes > 0
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(512))]

        /// A line's crossings found once serve every voxel on it, whichever voxel found
        /// them: the sign each voxel gets is the parity count by every triangle at its own
        /// point (ARCHITECTURE.md §5.1), lines through corners and along edges included, the
        /// corners and the points on a lattice of half steps, and points beyond the mesh's
        /// box along a line.
        #[test]
        fn a_lines_crossings_serve_every_voxel_on_it(
            corners in proptest::collection::vec(
                proptest::array::uniform3(proptest::array::uniform3(-3_i32..=3)),
                1..12,
            ),
            order in proptest::collection::vec(proptest::array::uniform3(-9_i32..=9), 1..200),
        ) {
            let m: Vec<[Point; 3]> =
                corners.iter().map(|t| t.map(|c| c.map(f64::from))).collect();
            let indexed = (0..m.len() * 3).map(|i| u32::try_from(i).unwrap()).collect::<Vec<_>>();
            let mesh = Mesh::new(
                m.iter().flatten().copied().collect(),
                indexed.chunks(3).map(|c| [c[0], c[1], c[2]]).collect(),
            );
            let bvh = Bvh::new(&mesh).unwrap();
            let mut lines = Lines::new(&bvh);
            for v in order {
                let p = v.map(|c| f64::from(c) * 0.5);
                let want = scanned(&m, p);
                proptest::prop_assert_eq!(lines.inside(v, p), want, "{:?}", v);
                proptest::prop_assert_eq!(inside(&bvh, p), want, "{:?}", v);
            }
        }
    }

    /// Two quads across one axis and none across another: the votes tie, and a tie is
    /// outside; a line crossing an odd number of times, through the third axis's one quad,
    /// does not vote (W1 of P5's review).
    #[test]
    fn tied_votes_are_outside_and_an_odd_line_does_not_vote() {
        // A unit square in the plane across axis `k` at `at`, as two triangles.
        let quad = |k: usize, at: f64, v: &mut Vec<Point>, t: &mut Vec<[u32; 3]>| {
            let base = u32::try_from(v.len()).unwrap();
            for (u, w) in [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)] {
                let mut p = [0.0; 3];
                (p[k], p[(k + 1) % 3], p[(k + 2) % 3]) = (at, u, w);
                v.push(p);
            }
            t.extend([[base, base + 1, base + 2], [base, base + 2, base + 3]]);
        };
        let (mut v, mut t) = (Vec::new(), Vec::new());
        quad(0, -1.0, &mut v, &mut t);
        quad(0, 1.0, &mut v, &mut t);
        quad(1, 1.0, &mut v, &mut t);
        let bvh = Bvh::new(&Mesh::new(v, t)).unwrap();
        // x: two crossings, one above, inside; y: one, no vote; z: none, outside.
        assert!(!inside(&bvh, [0.0, 0.0, 0.0]));
    }

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
