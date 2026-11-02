//! A shell: a sphere with a sphere's cavity inside it, the inner one facing in.

use colino::mesh::Mesh;

use crate::common::sphere;

/// A sphere of radius 20 about the origin, split `levels` times, holding a cavity of radius
/// `inner` about the same centre.
pub fn shell(levels: u32, inner: f64) -> Mesh {
    let mut m = sphere(levels, 20.0, [0.0; 3]);
    let hole = sphere(levels, inner, [0.0; 3]);
    let base = u32::try_from(m.vertices.len()).unwrap();
    m.vertices.extend(hole.vertices);
    m.triangles.extend(hole.triangles.iter().map(|[a, b, c]| [a + base, c + base, b + base]));
    m
}
