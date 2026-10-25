//! A tree of axis-aligned boxes over a mesh's triangles (ARCHITECTURE.md §5.1, ADR-0027),
//! for the triangle nearest a point. A node's triangles are split in two at the median of
//! their centres along the box's longest side, down to a few a leaf; building and walking
//! use an explicit stack. Of triangles at one distance, the lowest-numbered is nearest, so
//! that the answer does not depend on the walk; so too for the triangle nearest a segment.

use crate::distance::{Point, on_triangle, squared};
use crate::error::MeshError;
use crate::intersect::nearest;
use crate::mesh::Mesh;

/// The most triangles a leaf holds.
const LEAF: usize = 4;

/// A box, its smallest and largest corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Aabb {
    pub(crate) lo: Point,
    pub(crate) hi: Point,
}

impl Aabb {
    const EMPTY: Aabb = Aabb { lo: [f64::INFINITY; 3], hi: [f64::NEG_INFINITY; 3] };

    fn grow(self, p: Point) -> Aabb {
        let [l0, l1, l2] = self.lo;
        let [h0, h1, h2] = self.hi;
        Aabb {
            lo: [l0.min(p[0]), l1.min(p[1]), l2.min(p[2])],
            hi: [h0.max(p[0]), h1.max(p[1]), h2.max(p[2])],
        }
    }

    /// The squared distance from `p` to the box, zero inside it.
    fn squared_to(&self, p: Point) -> f64 {
        let gap = |k: usize, c: f64| {
            let (lo, hi) =
                (self.lo.get(k).copied().unwrap_or(0.0), self.hi.get(k).copied().unwrap_or(0.0));
            (lo - c).max(0.0).max(c - hi)
        };
        let [x, y, z] = [gap(0, p[0]), gap(1, p[1]), gap(2, p[2])];
        x * x + y * y + z * z
    }
}

/// A node: its box, the `count` triangles of `order` from `first` it holds, and its
/// children, numbered `children` and `children + 1`, or 0 for a leaf.
#[derive(Debug, Clone)]
pub(crate) struct Node {
    pub(crate) bounds: Aabb,
    pub(crate) first: usize,
    pub(crate) count: usize,
    pub(crate) children: usize,
}

/// A tree of boxes over a mesh's triangles, holding their corners.
#[derive(Debug, Clone)]
pub struct Bvh {
    pub(crate) nodes: Vec<Node>,
    /// Triangle numbers, ordered so that each node's are together.
    pub(crate) order: Vec<u32>,
    /// Each triangle's corners, by triangle number.
    pub(crate) corners: Vec<[Point; 3]>,
}

fn centre(t: &[Point; 3]) -> Point {
    let [a, b, c] = *t;
    [0, 1, 2].map(|k| {
        (a.get(k).copied().unwrap_or(0.0)
            + b.get(k).copied().unwrap_or(0.0)
            + c.get(k).copied().unwrap_or(0.0))
            / 3.0
    })
}

/// Each triangle's corners, refusing a triangle that names a vertex the mesh lacks.
fn corners(mesh: &Mesh) -> Result<Vec<[Point; 3]>, MeshError> {
    let mut out = Vec::with_capacity(mesh.triangles.len());
    for (triangle, t) in mesh.triangles.iter().enumerate() {
        let mut c = [[0.0; 3]; 3];
        for (corner, index) in c.iter_mut().zip(*t) {
            let at = usize::try_from(index).ok().and_then(|i| mesh.vertices.get(i));
            *corner = *at.ok_or(MeshError::BadIndex { triangle, index })?;
        }
        out.push(c);
    }
    Ok(out)
}

/// The box's longest side: 0, 1 or 2 for x, y or z, the first of equal ones.
fn longest(b: &Aabb) -> usize {
    let [x, y, z] = [0, 1, 2]
        .map(|k| b.hi.get(k).copied().unwrap_or(0.0) - b.lo.get(k).copied().unwrap_or(0.0));
    if x >= y && x >= z {
        0
    } else if y >= z {
        1
    } else {
        2
    }
}

impl Bvh {
    /// The tree over `mesh`'s triangles; a triangle naming a vertex the mesh lacks is
    /// refused.
    pub fn new(mesh: &Mesh) -> Result<Bvh, MeshError> {
        let corners = corners(mesh)?;
        let mut order: Vec<u32> =
            (0..u32::try_from(corners.len()).map_err(|_| MeshError::TooManyVertices)?).collect();
        let mut nodes = Vec::new();
        // (node, first, count) still to split.
        let mut stack = vec![(0_usize, 0_usize, order.len())];
        nodes.push(Node { bounds: Aabb::EMPTY, first: 0, count: 0, children: 0 });
        while let Some((id, first, count)) = stack.pop() {
            let range = first..first + count;
            let mine = order.get(range.clone()).unwrap_or(&[]);
            let tri = |i: &u32| {
                corners
                    .get(usize::try_from(*i).unwrap_or(usize::MAX))
                    .copied()
                    .unwrap_or([[0.0; 3]; 3])
            };
            let bounds = mine.iter().flat_map(&tri).fold(Aabb::EMPTY, Aabb::grow);
            let leaf = count <= LEAF;
            let children = nodes.len();
            if let Some(node) = nodes.get_mut(id) {
                *node = Node { bounds, first, count, children: if leaf { 0 } else { children } };
            }
            if leaf {
                continue;
            }
            let axis = longest(&bounds);
            let half = count / 2;
            if let Some(mine) = order.get_mut(range) {
                mine.select_nth_unstable_by(half, |x, y| {
                    let (cx, cy) = (centre(&tri(x)), centre(&tri(y)));
                    cx.get(axis)
                        .partial_cmp(&cy.get(axis))
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then(x.cmp(y))
                });
            }
            nodes.push(Node { bounds: Aabb::EMPTY, first: 0, count: 0, children: 0 });
            nodes.push(Node { bounds: Aabb::EMPTY, first: 0, count: 0, children: 0 });
            stack.push((children + 1, first + half, count - half));
            stack.push((children, first, half));
        }
        Ok(Bvh { nodes, order, corners })
    }

    /// The distance from `p` to the nearest triangle, and that triangle's number: of
    /// triangles at one distance, the lowest-numbered. Nothing for a mesh with no triangle.
    pub fn nearest(&self, p: Point) -> Option<(f64, u32)> {
        let mut best: Option<(f64, u32)> = None;
        let mut stack = vec![0_usize];
        while let Some(id) = stack.pop() {
            let Some(node) = self.nodes.get(id) else { continue };
            let reach = node.bounds.squared_to(p);
            if best.is_some_and(|(d, _)| reach > d) {
                continue;
            }
            if node.children == 0 {
                for i in self.order.get(node.first..node.first + node.count).unwrap_or(&[]) {
                    let Some(t) = self.corners.get(usize::try_from(*i).unwrap_or(usize::MAX))
                    else {
                        continue;
                    };
                    let d = squared(p, on_triangle(p, *t));
                    if best.is_none_or(|(b, j)| d.total_cmp(&b).then(i.cmp(&j)).is_lt()) {
                        best = Some((d, *i));
                    }
                }
                continue;
            }
            // The nearer child last, so that it is walked first.
            let (a, b) = (node.children, node.children + 1);
            let near =
                |c: usize| self.nodes.get(c).map_or(f64::INFINITY, |n| n.bounds.squared_to(p));
            if near(a) <= near(b) {
                stack.extend([b, a]);
            } else {
                stack.extend([a, b]);
            }
        }
        best.map(|(d, i)| (d.sqrt(), i))
    }

    /// The triangles whose boxes meet the box of the segment from `a` to `b`, in ascending
    /// order. The box is widened by twice the rounding of moving a corner by `a`, as the
    /// crossing test does, so that no triangle it would hit is left out (ADR-0030).
    pub fn near_segment(&self, a: Point, b: Point) -> Vec<u32> {
        let root = self.nodes.first().map_or(Aabb::EMPTY, |n| n.bounds);
        let reach = root
            .lo
            .iter()
            .chain(&root.hi)
            .chain(&a)
            .chain(&b)
            .fold(0.0_f64, |m, c| if c.is_finite() { m.max(c.abs()) } else { m });
        let margin = 4.0 * f64::EPSILON * reach;
        let query = Aabb::EMPTY.grow(a).grow(b);
        let query = Aabb { lo: query.lo.map(|c| c - margin), hi: query.hi.map(|c| c + margin) };
        let meets = |x: &Aabb| (0..3).all(|k| x.lo[k] <= query.hi[k] && query.lo[k] <= x.hi[k]);
        let mut out = Vec::new();
        let mut stack = vec![0_usize];
        while let Some(id) = stack.pop() {
            let Some(node) = self.nodes.get(id) else { continue };
            if !meets(&node.bounds) {
                continue;
            }
            if node.children != 0 {
                stack.extend([node.children, node.children + 1]);
                continue;
            }
            for i in self.order.get(node.first..node.first + node.count).unwrap_or(&[]) {
                let Some(t) = self.corners.get(usize::try_from(*i).unwrap_or(usize::MAX)) else {
                    continue;
                };
                if meets(&t.iter().fold(Aabb::EMPTY, |x, p| x.grow(*p))) {
                    out.push(*i);
                }
            }
        }
        out.sort_unstable();
        out
    }

    /// Of the triangles `keep` accepts, the one nearest the segment from `a` to `b` by
    /// `intersect::nearest`: the fraction of the segment's point nearest it, their squared
    /// distance, and its number, the lowest-numbered of equal distances, as a scan of every
    /// triangle in ascending order finds. A node is passed only when its box lies farther
    /// from the segment's box than the best distance so far by more than the rounding of
    /// distances at the tree's size, so that no triangle the scan would choose is left out.
    pub(crate) fn nearest_segment(
        &self,
        a: Point,
        b: Point,
        keep: impl Fn([Point; 3]) -> bool,
    ) -> Option<(f64, f64, u32)> {
        let root = self.nodes.first().map_or(Aabb::EMPTY, |n| n.bounds);
        let reach = root
            .lo
            .iter()
            .chain(&root.hi)
            .chain(&a)
            .chain(&b)
            .fold(0.0_f64, |m, c| if c.is_finite() { m.max(c.abs()) } else { m });
        let margin = 64.0 * f64::EPSILON * reach;
        let segment = Aabb::EMPTY.grow(a).grow(b);
        // The distance between two boxes, zero where they meet.
        let gap = |x: &Aabb| {
            let g = |k: usize| (x.lo[k] - segment.hi[k]).max(segment.lo[k] - x.hi[k]).max(0.0);
            let [p, q, r] = [g(0), g(1), g(2)];
            (p * p + q * q + r * r).sqrt()
        };
        let mut best: Option<(f64, f64, u32)> = None;
        let mut stack = vec![0_usize];
        while let Some(id) = stack.pop() {
            let Some(node) = self.nodes.get(id) else { continue };
            if best.is_some_and(|(_, d, _)| gap(&node.bounds) > d.sqrt() + margin) {
                continue;
            }
            if node.children == 0 {
                for i in self.order.get(node.first..node.first + node.count).unwrap_or(&[]) {
                    let Some(t) = self.corners(*i).filter(|t| keep(*t)) else { continue };
                    let (s, d) = nearest(a, b, t);
                    if best.is_none_or(|(_, e, j)| d.total_cmp(&e).then(i.cmp(&j)).is_lt()) {
                        best = Some((s, d, *i));
                    }
                }
                continue;
            }
            // The nearer child last, so that it is walked first.
            let (l, r) = (node.children, node.children + 1);
            let near = |c: usize| self.nodes.get(c).map_or(f64::INFINITY, |n| gap(&n.bounds));
            if near(l) <= near(r) {
                stack.extend([r, l]);
            } else {
                stack.extend([l, r]);
            }
        }
        best
    }

    /// Triangle `i`'s corners.
    pub fn corners(&self, i: u32) -> Option<[Point; 3]> {
        self.corners.get(usize::try_from(i).ok()?).copied()
    }

    /// The number of triangles.
    pub fn len(&self) -> u32 {
        u32::try_from(self.corners.len()).unwrap_or(u32::MAX)
    }

    /// Whether the mesh has no triangle.
    pub fn is_empty(&self) -> bool {
        self.corners.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixed linear congruential sequence of values in [−20, 20).
    fn sequence() -> impl FnMut() -> f64 {
        let mut g = 0x9e37_79b9_7f4a_7c15_u64;
        move || {
            g = g.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            f64::from(u32::try_from(g >> 40).unwrap_or(0)) / f64::from(1_u32 << 24) * 40.0 - 20.0
        }
    }

    /// Scattered triangles, every seventh repeated so that distances tie, every
    /// seventeenth with no area.
    fn scattered(next: &mut impl FnMut() -> f64) -> Mesh {
        let (mut vertices, mut triangles) = (Vec::new(), Vec::new());
        for i in 0..300_u32 {
            let c = [next(), next(), next()];
            let corner = |d: [f64; 3]| [c[0] + d[0], c[1] + d[1], c[2] + d[2]];
            let far = if i % 17 == 0 { [2.0, 0.0, 0.0] } else { [0.0, 1.5, 0.5] };
            vertices.extend([c, corner([1.0, 0.0, 0.0]), corner(far)]);
            triangles.push([3 * i, 3 * i + 1, 3 * i + 2]);
            if i % 7 == 0 {
                triangles.push([3 * i, 3 * i + 1, 3 * i + 2]);
            }
        }
        Mesh { vertices, triangles, ..Mesh::default() }
    }

    fn has_area(t: [Point; 3]) -> bool {
        let (u, v) = ([0, 1, 2].map(|k| t[1][k] - t[0][k]), [0, 1, 2].map(|k| t[2][k] - t[0][k]));
        let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        n.iter().any(|x| *x != 0.0)
    }

    /// The tree's nearest triangle to a segment is the one a scan of every triangle in
    /// ascending order finds, ties and triangles `keep` refuses among them, for segments
    /// near and far.
    #[test]
    fn the_nearest_to_a_segment_is_the_scans() {
        let mut next = sequence();
        let bvh = Bvh::new(&scattered(&mut next)).unwrap();
        let bits = |(s, d, i): (f64, f64, u32)| (s.to_bits(), d.to_bits(), i);
        let mut ties = 0;
        for _ in 0..400 {
            let a = [next(), next(), next()].map(|x| x * 1.5);
            let b = [0, 1, 2].map(|k| a[k] + next() / 10.0);
            let mut scan: Option<(f64, f64, u32)> = None;
            for i in 0..bvh.len() {
                let Some(t) = bvh.corners(i).filter(|t| has_area(*t)) else { continue };
                let (s, d) = nearest(a, b, t);
                if scan.is_none_or(|(_, e, _)| d < e) {
                    scan = Some((s, d, i));
                }
            }
            let found = bvh.nearest_segment(a, b, has_area);
            assert_eq!(found.map(bits), scan.map(bits), "{a:?} {b:?}");
            let (_, d, i) = found.unwrap();
            ties += usize::from((0..bvh.len()).any(|j| {
                let equal = |t: [Point; 3]| nearest(a, b, t).1.to_bits() == d.to_bits();
                j != i && bvh.corners(j).is_some_and(|t| has_area(t) && equal(t))
            }));
        }
        assert!(ties > 0, "no tie was decided");
    }

    /// Searching a small tree ends: a mutant that makes the walk loop for ever fails here
    /// within a minute, in the first test binary, before the tests that would only wait
    /// for it (TESTING.md T11). The segment's box meets the faces at z = 0 and x + y + z = 1.
    #[test]
    fn searching_a_tetrahedron_ends_in_time() {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let m = Mesh {
                vertices: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
                ..Mesh::default()
            };
            let found = Bvh::new(&m).map(|b| b.near_segment([0.2, 0.2, -1.0], [0.2, 0.2, 1.0]));
            tx.send(found).unwrap_or(());
        });
        assert_eq!(rx.recv_timeout(std::time::Duration::from_secs(60)), Ok(Ok(vec![0, 3])));
    }
}
