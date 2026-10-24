//! Meshes the tests convert: a sphere and a torus, closed and facing out.

use colino::distance::Point;
use colino::mesh::Mesh;

/// The octahedron split `levels` times onto a sphere of radius `r` about `centre`.
pub fn sphere(levels: u32, r: f64, centre: Point) -> Mesh {
    let mut m = Mesh::new(
        vec![
            [1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
        ],
        vec![
            [0, 2, 4],
            [2, 1, 4],
            [1, 3, 4],
            [3, 0, 4],
            [2, 0, 5],
            [1, 2, 5],
            [3, 1, 5],
            [0, 3, 5],
        ],
    )
    .with_unit(Some("millimeter".into()));
    for _ in 0..levels {
        let mut triangles = Vec::new();
        for [a, b, c] in m.triangles.clone() {
            let mut mid = |u: u32, v: u32| {
                let (p, q) = (m.vertices[u as usize], m.vertices[v as usize]);
                let s = [(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0, (p[2] + q[2]) / 2.0];
                let n = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt();
                m.vertices.push(s.map(|x| x / n));
                u32::try_from(m.vertices.len() - 1).unwrap()
            };
            let (ab, bc, ca) = (mid(a, b), mid(b, c), mid(c, a));
            triangles.extend([[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]]);
        }
        m.triangles = triangles;
    }
    for v in &mut m.vertices {
        *v = [0, 1, 2].map(|k| centre[k] + r * v[k]);
    }
    m
}

/// A torus about the z axis, ring radius `big`, tube `small`, `n` by `n` quads split.
pub fn torus(big: f64, small: f64, n: u32) -> Mesh {
    let angle = |i: u32| 2.0 * std::f64::consts::PI * f64::from(i % n) / f64::from(n);
    let mut vertices = Vec::new();
    for i in 0..n {
        for j in 0..n {
            let (u, v) = (angle(i), angle(j));
            let ring = big + small * v.cos();
            vertices.push([ring * u.cos(), ring * u.sin(), small * v.sin()]);
        }
    }
    let at = |i: u32, j: u32| (i % n) * n + j % n;
    let mut triangles = Vec::new();
    for i in 0..n {
        for j in 0..n {
            triangles.push([at(i, j), at(i + 1, j), at(i + 1, j + 1)]);
            triangles.push([at(i, j), at(i + 1, j + 1), at(i, j + 1)]);
        }
    }
    Mesh::new(vertices, triangles).with_unit(Some("millimeter".into()))
}
