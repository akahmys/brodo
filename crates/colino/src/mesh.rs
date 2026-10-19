//! A triangle mesh as read from a file (ARCHITECTURE.md §5.1): vertices and the triangles
//! that index them, as given. Nothing is repaired; badly oriented, holed and degenerate
//! meshes are kept as they are (ADR-0017).

use std::collections::BTreeMap;

use crate::error::MeshError;

/// A triangle mesh.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mesh {
    /// Vertex positions.
    pub vertices: Vec<[f64; 3]>,
    /// Triangles, each three indices into `vertices`, in the file's order.
    pub triangles: Vec<[u32; 3]>,
    /// The unit of length the file states, uninterpreted; none when it states none.
    pub unit: Option<String>,
}

/// The most a reader takes from an untrusted file; beyond them it fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// The most triangles.
    pub triangles: u64,
    /// The most vertices.
    pub vertices: u64,
    /// The most bytes read: a file's own, or each part a package holds, once expanded.
    pub bytes: u64,
}

impl Limits {
    /// Refuses `len` bytes beyond the limit.
    pub(crate) fn check_bytes(&self, len: u64) -> Result<(), crate::error::ReadError> {
        if len > self.bytes {
            return Err(crate::error::ReadError::TooLarge { what: "bytes", limit: self.bytes });
        }
        Ok(())
    }
}

impl Mesh {
    /// The same mesh with equal positions as one vertex, numbered in the order each first
    /// appears; triangles keep their order and orientation. Positions are equal when their
    /// coordinates are: `-0.0` and `0.0` are one, nothing within a tolerance is. A file that
    /// lists each triangle's corners apart, as STL does, needs this for its connectivity.
    pub fn welded(&self) -> Result<Mesh, MeshError> {
        let key = |p: [f64; 3]| p.map(|c| (c + 0.0).to_bits());
        let mut seen: BTreeMap<[u64; 3], u32> = BTreeMap::new();
        let mut vertices = Vec::new();
        let mut renumber = Vec::with_capacity(self.vertices.len());
        for p in &self.vertices {
            let next = u32::try_from(vertices.len()).map_err(|_| MeshError::TooManyVertices)?;
            let id = *seen.entry(key(*p)).or_insert_with(|| {
                vertices.push(*p);
                next
            });
            renumber.push(id);
        }
        let mut triangles = Vec::with_capacity(self.triangles.len());
        for (triangle, t) in self.triangles.iter().enumerate() {
            let mut out = [0; 3];
            for (o, index) in out.iter_mut().zip(*t) {
                let at = usize::try_from(index).ok().and_then(|i| renumber.get(i));
                *o = *at.ok_or(MeshError::BadIndex { triangle, index })?;
            }
            triangles.push(out);
        }
        Ok(Mesh { vertices, triangles, unit: self.unit.clone() })
    }
}
