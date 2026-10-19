//! Benchmark tier (TESTING.md §1) for colino, on brodo's harness (`harness/mod.rs`,
//! ADR-0019): `cargo bench -p colino --bench colino`, `--record` to record a baseline.
//!
//! The workload is a sphere of radius 50 voxels, the octahedron split five times (8192
//! triangles, edges near 2.5 voxels), converted with a band of 3 and labelled. At a
//! radius of 400 the conversion took 33 s and labelling 9.5 s (2026-10-20, ADR-0027);
//! these items watch the same paths at a size that runs in seconds.

use brodo::analysis::{Connectivity, label};
use colino::convert::{Converted, Options, convert};
use colino::mesh::Mesh;

#[path = "../../brodo/benches/harness/mod.rs"]
mod harness;
use harness::{Failure, Item, repeated};

/// The octahedron split `levels` times onto a sphere of radius `r`, its centre off the
/// lattice so that no vertex lies on a voxel's boundary.
fn sphere(levels: u32, r: f64) -> Mesh {
    let mut m = Mesh {
        vertices: vec![
            [1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
        ],
        triangles: vec![
            [0, 2, 4],
            [2, 1, 4],
            [1, 3, 4],
            [3, 0, 4],
            [2, 0, 5],
            [1, 2, 5],
            [3, 1, 5],
            [0, 3, 5],
        ],
        unit: Some("millimeter".into()),
    };
    for _ in 0..levels {
        let mut midpoints = std::collections::BTreeMap::new();
        let mut triangles = Vec::with_capacity(m.triangles.len() * 4);
        for [a, b, c] in m.triangles.clone() {
            let mut mid = |u: u32, v: u32| {
                *midpoints.entry((u.min(v), u.max(v))).or_insert_with(|| {
                    let (p, q) = (m.vertices[u as usize], m.vertices[v as usize]);
                    let s = [(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0, (p[2] + q[2]) / 2.0];
                    let n = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt();
                    m.vertices.push(s.map(|x| x / n));
                    u32::try_from(m.vertices.len() - 1).unwrap_or(u32::MAX)
                })
            };
            let (ab, bc, ca) = (mid(a, b), mid(b, c), mid(c, a));
            triangles.extend([[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]]);
        }
        m.triangles = triangles;
    }
    for v in &mut m.vertices {
        *v = v.map(|x| x * r + 0.3);
    }
    m
}

/// The mesh, and its conversion for the labelling item.
struct Data {
    mesh: Mesh,
    converted: Converted,
}

impl Data {
    fn new() -> Result<Self, Failure> {
        let mesh = sphere(5, 50.0);
        let converted = convert(&mesh, &Options::new(1.0))?;
        Ok(Data { mesh, converted })
    }

    fn items(&self) -> Vec<Item<'_>> {
        let c = &self.converted;
        vec![
            repeated(
                "convert_sphere_r50",
                1,
                Box::new(|| {
                    Ok(convert(&self.mesh, &Options::new(1.0))?.topology.leaf_count() as f64)
                }),
            ),
            repeated(
                "label_sphere_r50",
                1,
                Box::new(|| {
                    Ok(f64::from(
                        label(&c.topology, &c.field, |v| v < 0.0, Connectivity::TwentySix)?.count(),
                    ))
                }),
            ),
        ]
    }
}

fn main() -> Result<(), Failure> {
    harness::run(env!("CARGO_MANIFEST_DIR"), "colino", Data::new, Data::items)
}
