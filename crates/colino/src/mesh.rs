//! A triangle mesh as read from a file (ARCHITECTURE.md §5.1): vertices and the triangles
//! that index them, as given. Nothing is repaired; badly oriented, holed and degenerate
//! meshes are kept as they are (ADR-0017).

use std::collections::BTreeMap;

use crate::error::MeshError;

/// A triangle mesh.
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct Mesh {
    /// Vertex positions.
    pub vertices: Vec<[f64; 3]>,
    /// Triangles, each three indices into `vertices`, in the file's order.
    pub triangles: Vec<[u32; 3]>,
    /// The unit of length the file states, uninterpreted; none when it states none.
    pub unit: Option<String>,
    /// Each triangle's group, in `triangles`' order; empty when every triangle is in group
    /// 0 (ADR-0030).
    pub groups: Vec<u32>,
    /// Each group's name, indexed by its id, the ids given in the order each name first
    /// appears. Group 0 is `""`, no group; empty, this stands for `[""]`.
    pub group_names: Vec<String>,
}

/// The most a reader takes from an untrusted file; beyond them it fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Limits {
    /// The most triangles.
    pub triangles: u64,
    /// The most vertices.
    pub vertices: u64,
    /// The most bytes read: a file's own, or each part a package holds, once expanded.
    pub bytes: u64,
}

impl Limits {
    /// At most `triangles` triangles, `vertices` vertices and `bytes` bytes.
    pub const fn new(triangles: u64, vertices: u64, bytes: u64) -> Limits {
        Limits { triangles, vertices, bytes }
    }

    /// The same limits, with at most `triangles` triangles.
    #[must_use]
    pub const fn with_triangles(self, triangles: u64) -> Limits {
        Limits { triangles, ..self }
    }

    /// The same limits, with at most `vertices` vertices.
    #[must_use]
    pub const fn with_vertices(self, vertices: u64) -> Limits {
        Limits { vertices, ..self }
    }

    /// The same limits, with at most `bytes` bytes.
    #[must_use]
    pub const fn with_bytes(self, bytes: u64) -> Limits {
        Limits { bytes, ..self }
    }

    /// Refuses `len` bytes beyond the limit.
    pub(crate) fn check_bytes(&self, len: u64) -> Result<(), crate::error::ReadError> {
        if len > self.bytes {
            return Err(crate::error::ReadError::TooLarge { what: "bytes", limit: self.bytes });
        }
        Ok(())
    }
}

/// Groups as a reader meets them: each name numbered in the order a triangle first takes
/// it, `""` always 0 (ADR-0030).
#[derive(Debug, Default)]
pub(crate) struct Grouping {
    ids: BTreeMap<String, u32>,
    names: Vec<String>,
    groups: Vec<u32>,
}

impl Grouping {
    /// Puts the next `n` triangles in the group named `name`.
    pub(crate) fn take(&mut self, name: &str, n: usize) {
        if n == 0 {
            return;
        }
        let id = if name.is_empty() {
            0
        } else if let Some(id) = self.ids.get(name) {
            *id
        } else {
            if self.names.is_empty() {
                self.names.push(String::new());
            }
            let id = u32::try_from(self.names.len()).unwrap_or(u32::MAX);
            self.ids.insert(name.to_owned(), id);
            self.names.push(name.to_owned());
            id
        };
        self.groups.extend(std::iter::repeat_n(id, n));
    }

    /// Gives `mesh` the groups taken, none when every triangle is in group 0.
    pub(crate) fn into(self, mesh: &mut Mesh) {
        if !self.names.is_empty() {
            mesh.groups = self.groups;
            mesh.group_names = self.names;
        }
    }
}

impl Mesh {
    /// A mesh of `vertices` and the `triangles` that index them, with no unit and no
    /// groups.
    pub fn new(vertices: Vec<[f64; 3]>, triangles: Vec<[u32; 3]>) -> Mesh {
        Mesh { vertices, triangles, ..Mesh::default() }
    }

    /// The same mesh, its unit `unit`.
    #[must_use]
    pub fn with_unit(self, unit: Option<String>) -> Mesh {
        Mesh { unit, ..self }
    }

    /// The same mesh, its triangles `triangles`.
    #[must_use]
    pub fn with_triangles(self, triangles: Vec<[u32; 3]>) -> Mesh {
        Mesh { triangles, ..self }
    }

    /// The same mesh, each triangle in its group of `groups`, the groups named by
    /// `group_names` (ADR-0030).
    #[must_use]
    pub fn with_groups(self, groups: Vec<u32>, group_names: Vec<String>) -> Mesh {
        Mesh { groups, group_names, ..self }
    }

    /// Refuses groups that do not fit the triangles: `groups` neither empty nor one per
    /// triangle, an id with no name, or a first name other than `""`.
    pub fn check_groups(&self) -> Result<(), MeshError> {
        if !self.groups.is_empty() && self.groups.len() != self.triangles.len() {
            return Err(MeshError::GroupCount {
                groups: self.groups.len(),
                triangles: self.triangles.len(),
            });
        }
        if self.group_names.first().is_some_and(|n| !n.is_empty()) {
            return Err(MeshError::GroupZeroNamed);
        }
        let named = self.group_names.len().max(1);
        let unnamed =
            self.groups.iter().position(|g| usize::try_from(*g).map_or(true, |g| g >= named));
        if let Some(triangle) = unnamed {
            let group = self.groups.get(triangle).copied().unwrap_or(0);
            return Err(MeshError::UnnamedGroup { triangle, group });
        }
        Ok(())
    }

    /// The group of triangle `t`: 0 when the mesh has no groups or no such triangle.
    pub fn group(&self, t: usize) -> u32 {
        self.groups.get(t).copied().unwrap_or(0)
    }

    /// The same mesh with equal positions as one vertex, numbered in the order each first
    /// appears; triangles keep their order and orientation. Positions are equal when their
    /// coordinates are: `-0.0` and `0.0` are one, nothing within a tolerance is. A file that
    /// lists each triangle's corners apart, as STL does, needs this for its connectivity.
    /// Each triangle keeps its group; groups that do not fit are refused.
    pub fn welded(&self) -> Result<Mesh, MeshError> {
        self.check_groups()?;
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
        Ok(Mesh {
            vertices,
            triangles,
            unit: self.unit.clone(),
            groups: self.groups.clone(),
            group_names: self.group_names.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two triangles over four vertices, the first two of them at one position.
    fn pair(groups: Vec<u32>, names: &[&str]) -> Mesh {
        Mesh {
            vertices: vec![[0.0; 3], [0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            triangles: vec![[0, 2, 3], [1, 3, 2]],
            unit: None,
            groups,
            group_names: names.iter().map(|n| (*n).to_string()).collect(),
        }
    }

    #[test]
    fn groups_that_fit_the_triangles_are_taken() {
        assert_eq!(pair(vec![], &[]).check_groups(), Ok(()));
        assert_eq!(pair(vec![0, 0], &[]).check_groups(), Ok(()));
        assert_eq!(pair(vec![1, 0], &["", "a"]).check_groups(), Ok(()));
        assert_eq!(pair(vec![], &["", "a"]).group(1), 0);
        assert_eq!(pair(vec![1, 2], &["", "a", "b"]).group(1), 2);
    }

    #[test]
    fn groups_that_do_not_fit_are_refused() {
        assert_eq!(
            pair(vec![0], &[]).check_groups(),
            Err(MeshError::GroupCount { groups: 1, triangles: 2 })
        );
        assert_eq!(
            pair(vec![0, 1], &[]).check_groups(),
            Err(MeshError::UnnamedGroup { triangle: 1, group: 1 })
        );
        assert_eq!(
            pair(vec![2, 1], &["", "a"]).check_groups(),
            Err(MeshError::UnnamedGroup { triangle: 0, group: 2 })
        );
        assert_eq!(pair(vec![], &["a"]).check_groups(), Err(MeshError::GroupZeroNamed));
        assert_eq!(
            pair(vec![0], &[]).welded(),
            Err(MeshError::GroupCount { groups: 1, triangles: 2 })
        );
    }

    #[test]
    fn welding_keeps_each_triangles_group() {
        let m = pair(vec![2, 1], &["", "a", "b"]);
        let w = m.welded().unwrap();
        assert_eq!(w.vertices.len(), 3);
        assert_eq!(w.triangles, vec![[0, 1, 2], [0, 2, 1]]);
        assert_eq!((w.groups, w.group_names), (m.groups, m.group_names));
    }
}
