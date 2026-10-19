//! The generalised winding number of a mesh at a point (ARCHITECTURE.md §5.1, ADR-0027):
//! the sum of its triangles' signed solid angles over 4π, after Jacobson, Kavan and
//! Sorkine-Hornung (2013); 1 inside a closed, outward-facing surface, 0 outside, and in
//! between where the surface is open or overlaps itself. A triangle's solid angle is van
//! Oosterom and Strackee's (their equation 6).
//!
//! The approximation is Barill, Dickson, Schmidt, Levin and Jacobson's (2018, Algorithm 1):
//! on the tree of boxes, a node farther than β times its radius is taken whole, by the
//! Taylor expansion to second order of the dipole field of its triangles about their
//! area-weighted centre (equations 18 to 20, coefficients by appendix B, derivatives by
//! appendix A); a nearer one is opened, and a leaf's triangles summed exactly.

use std::f64::consts::PI;

use crate::bvh::Bvh;
use crate::distance::Point;
use crate::error::MeshError;
use crate::mesh::Mesh;

fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: Point, b: Point) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: Point, b: Point) -> Point {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn norm(a: Point) -> f64 {
    dot(a, a).sqrt()
}

/// The signed solid angle of triangle `t` seen from `p`.
pub fn solid_angle(p: Point, t: [Point; 3]) -> f64 {
    let [a, b, c] = t.map(|v| sub(v, p));
    let (la, lb, lc) = (norm(a), norm(b), norm(c));
    let det = dot(a, cross(b, c));
    let den = la * lb * lc + dot(a, b) * lc + dot(b, c) * la + dot(c, a) * lb;
    2.0 * det.atan2(den)
}

/// A node's far-field expansion: about `centre`, holding every triangle within `radius`;
/// the area-weighted normals' sum, and their first and second moments about the centre.
#[derive(Debug, Clone, Copy)]
struct Expansion {
    centre: Point,
    radius: f64,
    normal: Point,
    first: [[f64; 3]; 3],
    second: [[[f64; 3]; 3]; 3],
}

/// Sums a triangle's corners, or the midpoints of its edges, less `c`, in fixed order.
fn shifted(points: [Point; 3], c: Point) -> [Point; 3] {
    points.map(|p| sub(p, c))
}

impl Expansion {
    fn of(triangles: &[[Point; 3]]) -> Expansion {
        let area = |t: &[Point; 3]| norm(cross(sub(t[1], t[0]), sub(t[2], t[0]))) / 2.0;
        let mid = |t: &[Point; 3]| [0, 1, 2].map(|k| (t[0][k] + t[1][k] + t[2][k]) / 3.0);
        let total: f64 = triangles.iter().map(area).sum();
        let mut centre = [0.0; 3];
        for t in triangles {
            // Weighted by area; by corners alone when the triangles have none.
            let (w, m) = if total > 0.0 {
                (area(t) / total, mid(t))
            } else {
                (1.0 / triangles.len() as f64, mid(t))
            };
            for k in 0..3 {
                centre[k] += w * m[k];
            }
        }
        let mut e = Expansion {
            centre,
            radius: 0.0,
            normal: [0.0; 3],
            first: [[0.0; 3]; 3],
            second: [[[0.0; 3]; 3]; 3],
        };
        for t in triangles {
            for v in t {
                e.radius = e.radius.max(norm(sub(*v, centre)));
            }
            // The area-weighted normal, half the edges' cross product.
            let an = cross(sub(t[1], t[0]), sub(t[2], t[0])).map(|x| x / 2.0);
            let d = sub(mid(t), centre);
            let edges = [[t[0], t[1]], [t[1], t[2]], [t[2], t[0]]]
                .map(|[u, v]| [0, 1, 2].map(|k| (u[k] + v[k]) / 2.0));
            let m = shifted(edges, centre);
            for i in 0..3 {
                e.normal[i] += an[i];
                for j in 0..3 {
                    e.first[i][j] += d[i] * an[j];
                    let c_ij = (m[0][i] * m[0][j] + m[1][i] * m[1][j] + m[2][i] * m[2][j]) / 3.0;
                    for (k, a) in an.iter().enumerate() {
                        e.second[i][j][k] += c_ij * a;
                    }
                }
            }
        }
        e
    }

    /// The expansion's winding number at `q`.
    fn at(&self, q: Point) -> f64 {
        let r = sub(self.centre, q);
        let d2 = dot(r, r);
        let d = d2.sqrt();
        let (g1, g3, g5, g7) = (
            4.0 * PI * d * d2,
            4.0 * PI * d * d2,
            4.0 * PI * d * d2 * d2,
            4.0 * PI * d * d2 * d2 * d2,
        );
        let mut w = dot(self.normal, r) / g1;
        for i in 0..3 {
            for j in 0..3 {
                let delta = if i == j { 1.0 } else { 0.0 };
                w += self.first[i][j] * (delta / g3 - 3.0 * r[i] * r[j] / g5);
                for k in 0..3 {
                    let (dij, djk, dik) =
                        (delta, if j == k { 1.0 } else { 0.0 }, if i == k { 1.0 } else { 0.0 });
                    let third = -3.0 * (r[i] * djk + r[j] * dik + r[k] * dij) / g5
                        + 15.0 * r[i] * r[j] * r[k] / g7;
                    w += 0.5 * self.second[i][j][k] * third;
                }
            }
        }
        w
    }
}

/// How the winding number is found.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Accuracy {
    /// Every triangle's solid angle, summed.
    Exact,
    /// Barill et al.'s approximation: a node farther than `beta` times its radius taken
    /// whole. They report β = 2 thresholds at 1/2 as the exact sum does.
    Approximate {
        /// The accuracy parameter, β.
        beta: f64,
    },
}

/// A mesh prepared for its winding number.
#[derive(Debug, Clone)]
pub struct Winding {
    bvh: Bvh,
    expansions: Vec<Expansion>,
}

impl Winding {
    /// Prepares `mesh`: its tree of boxes and each node's expansion. A triangle naming a
    /// vertex the mesh lacks is refused.
    pub fn new(mesh: &Mesh) -> Result<Winding, MeshError> {
        let bvh = Bvh::new(mesh)?;
        let expansions = bvh
            .nodes
            .iter()
            .map(|n| {
                let mine: Vec<[Point; 3]> = bvh
                    .order
                    .get(n.first..n.first + n.count)
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(|i| {
                        bvh.corners.get(usize::try_from(*i).unwrap_or(usize::MAX)).copied()
                    })
                    .collect();
                Expansion::of(&mine)
            })
            .collect();
        Ok(Winding { bvh, expansions })
    }

    /// The mesh's triangles, by their corners.
    pub(crate) fn triangles(&self) -> &[[Point; 3]] {
        &self.bvh.corners
    }

    /// The distance from `q` to the nearest triangle; nothing for a mesh with none.
    pub(crate) fn nearest(&self, q: Point) -> Option<f64> {
        self.bvh.nearest(q).map(|(d, _)| d)
    }

    /// The box holding every triangle, its least and greatest corner; nothing for a mesh
    /// with none.
    pub(crate) fn bounds(&self) -> Option<(Point, Point)> {
        let root = self.bvh.nodes.first().filter(|n| n.count > 0)?;
        Some((root.bounds.lo, root.bounds.hi))
    }

    /// The winding number at `q`.
    pub fn number(&self, q: Point, accuracy: Accuracy) -> f64 {
        match accuracy {
            Accuracy::Exact => {
                self.bvh.corners.iter().map(|t| solid_angle(q, *t)).sum::<f64>() / (4.0 * PI)
            }
            Accuracy::Approximate { beta } => self.approximate(q, beta),
        }
    }

    fn approximate(&self, q: Point, beta: f64) -> f64 {
        let mut w = 0.0;
        let mut stack = vec![0_usize];
        while let Some(id) = stack.pop() {
            let (Some(node), Some(e)) = (self.bvh.nodes.get(id), self.expansions.get(id)) else {
                continue;
            };
            if node.count == 0 {
                continue;
            }
            if norm(sub(q, e.centre)) > beta * e.radius {
                w += e.at(q);
            } else if node.children == 0 {
                let ids = self.bvh.order.get(node.first..node.first + node.count).unwrap_or(&[]);
                for i in ids {
                    if let Some(t) = self.bvh.corners.get(usize::try_from(*i).unwrap_or(usize::MAX))
                    {
                        w += solid_angle(q, *t) / (4.0 * PI);
                    }
                }
            } else {
                stack.extend([node.children + 1, node.children]);
            }
        }
        w
    }
}
