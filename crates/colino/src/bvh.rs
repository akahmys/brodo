//! A tree of axis-aligned boxes over a mesh's triangles (ARCHITECTURE.md §5.1, ADR-0027),
//! for the triangle nearest a point. A node's triangles are split in two at the median of
//! their centres along the box's longest side, down to a few a leaf; building and walking
//! use an explicit stack. Of triangles at one distance, the lowest-numbered is nearest, so
//! that the answer does not depend on the walk.

use crate::distance::{Point, on_triangle, squared};
use crate::error::MeshError;
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

/// A node: its box, and either its children, numbered `children` and `children + 1`, or
/// the `count` triangles of `order` from `first`.
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
                *node = Node {
                    bounds,
                    first,
                    count: if leaf { count } else { 0 },
                    children: if leaf { 0 } else { children },
                };
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
}
