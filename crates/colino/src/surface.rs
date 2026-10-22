//! A mesh's surface as the topology report's reference (ARCHITECTURE.md §5.2, ADR-0028):
//! its vertices, edges and faces, and whether it is a closed 2-manifold, every edge shared
//! by exactly two triangles in opposite directions, every vertex's triangles one fan, no
//! triangle with a repeated corner. Only then is V − E + F the surface's Euler
//! characteristic and half of it the solid's. Weld a mesh that lists triangles' corners
//! apart first (`Mesh::welded`).

use std::collections::{BTreeMap, BTreeSet};

use crate::error::MeshError;
use crate::mesh::Mesh;

/// A mesh's surface, counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Surface {
    /// Vertices some triangle uses.
    pub vertices: u64,
    /// Edges, each pair of vertices some triangle joins.
    pub edges: u64,
    /// Triangles, those with a repeated corner left out.
    pub faces: u64,
    /// Edges of one triangle: holes' rims.
    pub open_edges: u64,
    /// Edges of three triangles or more.
    pub crowded_edges: u64,
    /// Edges of two triangles that run them the same way: the triangles disagree in
    /// orientation.
    pub misoriented_edges: u64,
    /// Vertices whose triangles fall into more than one fan, as where two shapes touch at
    /// a point.
    pub fanless_vertices: u64,
    /// Triangles with a repeated corner.
    pub degenerate_triangles: u64,
}

impl Surface {
    /// Counts `mesh`'s surface; a triangle naming a vertex the mesh lacks is refused.
    pub fn of(mesh: &Mesh) -> Result<Surface, MeshError> {
        let n = mesh.vertices.len();
        let mut s = Surface::default();
        let mut used = BTreeSet::new();
        // Each edge, lower vertex first: triangles running it upward and downward.
        let mut edges: BTreeMap<(u32, u32), [u64; 2]> = BTreeMap::new();
        let mut faces = Vec::new();
        for (triangle, t) in mesh.triangles.iter().enumerate() {
            if let Some(index) =
                t.iter().copied().find(|i| usize::try_from(*i).map_or(true, |i| i >= n))
            {
                return Err(MeshError::BadIndex { triangle, index });
            }
            let [a, b, c] = *t;
            if a == b || b == c || c == a {
                s.degenerate_triangles += 1;
                continue;
            }
            faces.push(*t);
            used.extend([a, b, c]);
            for (u, v) in [(a, b), (b, c), (c, a)] {
                let runs = edges.entry((u.min(v), u.max(v))).or_insert([0, 0]);
                if let Some(r) = runs.get_mut(usize::from(u > v)) {
                    *r += 1;
                }
            }
        }
        s.faces = faces.len() as u64;
        s.vertices = used.len() as u64;
        s.edges = edges.len() as u64;
        for [up, down] in edges.values() {
            match up + down {
                1 => s.open_edges += 1,
                2 if *up != 1 => s.misoriented_edges += 1,
                2 => {}
                _ => s.crowded_edges += 1,
            }
        }
        s.fanless_vertices = fanless(&faces);
        Ok(s)
    }

    /// Whether the surface is a closed 2-manifold, so that V − E + F is its Euler
    /// characteristic.
    pub fn closed_manifold(&self) -> bool {
        self.open_edges == 0
            && self.crowded_edges == 0
            && self.misoriented_edges == 0
            && self.fanless_vertices == 0
            && self.degenerate_triangles == 0
    }

    /// V − E + F.
    pub fn euler(&self) -> i64 {
        let signed = |x: u64| i64::try_from(x).unwrap_or(i64::MAX);
        signed(self.vertices) - signed(self.edges) + signed(self.faces)
    }
}

/// The root of `x` in a union-find forest, halving paths on the way.
fn root(parent: &mut [usize], mut x: usize) -> usize {
    while let Some(&p) = parent.get(x) {
        if p == x {
            break;
        }
        let grand = parent.get(p).copied().unwrap_or(p);
        if let Some(slot) = parent.get_mut(x) {
            *slot = grand;
        }
        x = grand;
    }
    x
}

/// The vertices whose triangles fall into more than one fan: triangles around a vertex are
/// one fan when they are joined, one to the next, by edges through the vertex. Each
/// vertex's triangles are joined apart from every other vertex's.
fn fanless(faces: &[[u32; 3]]) -> u64 {
    // Each vertex's triangles, with the other two corners of each.
    let mut around: BTreeMap<u32, Vec<[u32; 2]>> = BTreeMap::new();
    for [a, b, c] in faces {
        for (v, x, y) in [(*a, *b, *c), (*b, *c, *a), (*c, *a, *b)] {
            around.entry(v).or_default().push([x, y]);
        }
    }
    let mut count = 0;
    for triangles in around.values() {
        let mut parent: Vec<usize> = (0..triangles.len()).collect();
        let mut first: BTreeMap<u32, usize> = BTreeMap::new();
        for (f, others) in triangles.iter().enumerate() {
            for other in others {
                match first.get(other) {
                    Some(&g) => {
                        let (ra, rb) = (root(&mut parent, f), root(&mut parent, g));
                        if let Some(slot) = parent.get_mut(ra.max(rb)) {
                            *slot = ra.min(rb);
                        }
                    }
                    None => {
                        first.insert(*other, f);
                    }
                }
            }
        }
        let fans: BTreeSet<usize> = (0..triangles.len()).map(|f| root(&mut parent, f)).collect();
        count += u64::from(fans.len() > 1);
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Counting a small surface ends: a mutant that makes it loop for ever fails here within
    /// a minute, in the first test binary, before the tests that would only wait for it
    /// (TESTING.md T11).
    #[test]
    fn counting_a_tetrahedron_ends_in_time() {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let m = Mesh {
                vertices: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
                unit: None,
            };
            tx.send(Surface::of(&m).map(|s| s.closed_manifold())).unwrap_or(());
        });
        assert_eq!(rx.recv_timeout(std::time::Duration::from_secs(60)), Ok(Ok(true)));
    }
}
