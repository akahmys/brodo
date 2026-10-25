//! Benchmark tier (TESTING.md §1) for colino, on brodo's harness (`harness/mod.rs`,
//! ADR-0019): `cargo bench -p colino --bench colino`, `--record` to record a baseline,
//! `--record NAME` one item's.
//!
//! The workload is a sphere of radius 50 voxels, the octahedron split five times (8192
//! triangles, edges near 2.5 voxels), converted with a band of 3 and labelled. At a
//! radius of 400 the conversion took 33 s and labelling 9.5 s (2026-10-20, ADR-0027);
//! these items watch the same paths at a size that runs in seconds.
//!
//! Hermite data is made from the same conversion, and from the sphere with a cap of 128
//! triangles removed, a hole about 20 voxels across: 537 of its edges hit no triangle
//! (2026-10-25). A hole of 232 triangles changes the signs beyond the band, which Hermite
//! data refuses (ADR-0030). A slab, 320 by 320 by 6 voxels and turned about two axes, is
//! twelve large slanted triangles: its conversion's candidate cells are the triangles'
//! boxes rather than their areas.

use brodo::analysis::{Connectivity, label};
use colino::convert::{Converted, Options, convert};
use colino::hermite::hermite;
use colino::mesh::Mesh;

#[path = "../../brodo/benches/harness/mod.rs"]
mod harness;
use harness::{Failure, Item, repeated};

/// The octahedron split `levels` times onto a sphere of radius `r`, its centre off the
/// lattice so that no vertex lies on a voxel's boundary.
fn sphere(levels: u32, r: f64) -> Mesh {
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

/// The sphere with the triangles whose centre lies above `z` removed.
fn capless(m: &Mesh, z: f64) -> Mesh {
    let mut holed = m.clone();
    let centre_z = |t: &[u32; 3]| t.iter().map(|i| m.vertices[*i as usize][2]).sum::<f64>() / 3.0;
    holed.triangles.retain(|t| centre_z(t) <= z);
    holed
}

/// A box `size` voxels on a side, facing out, turned about x and then z and moved off the
/// lattice.
fn slab(size: [f64; 3]) -> Mesh {
    let (a, b) = (0.5_f64, 0.3_f64);
    let turn = |corner: [f64; 3]| {
        let [x, y, z] = [0, 1, 2].map(|k| (corner[k] - 0.5) * size[k]);
        let (y, z) = (y * a.cos() - z * a.sin(), y * a.sin() + z * a.cos());
        let (x, y) = (x * b.cos() - y * b.sin(), x * b.sin() + y * b.cos());
        [x + 0.3, y + 0.1, z - 0.2]
    };
    let vertices = (0..8).map(|i| turn([0, 1, 2].map(|k| f64::from(i >> k & 1)))).collect();
    // Corner i has bit k set at the high end of axis k; each face's two triangles turn
    // counter-clockwise seen from outside.
    let triangles = vec![
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
    Mesh::new(vertices, triangles).with_unit(Some("millimeter".into()))
}

/// The meshes, and the conversions the labelling and Hermite items start from.
struct Data {
    mesh: Mesh,
    converted: Converted,
    holed: Mesh,
    holed_converted: Converted,
    slab: Mesh,
}

impl Data {
    fn new() -> Result<Self, Failure> {
        let o = Options::new(1.0);
        let mesh = sphere(5, 50.0);
        let converted = convert(&mesh, &o)?;
        let holed = capless(&mesh, 49.0);
        let holed_converted = convert(&holed, &o)?;
        Ok(Data { mesh, converted, holed, holed_converted, slab: slab([320.0, 320.0, 6.0]) })
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
            repeated(
                "hermite_sphere_r50",
                1,
                Box::new(|| {
                    let o = Options::new(1.0);
                    Ok(hermite(&self.mesh, c, o.accuracy)?.unhit as f64)
                }),
            ),
            repeated(
                "hermite_holed_r50",
                1,
                Box::new(|| {
                    let o = Options::new(1.0);
                    Ok(hermite(&self.holed, &self.holed_converted, o.accuracy)?.unhit as f64)
                }),
            ),
            repeated(
                "convert_slab_320",
                1,
                Box::new(|| {
                    Ok(convert(&self.slab, &Options::new(1.0))?.topology.leaf_count() as f64)
                }),
            ),
        ]
    }
}

fn main() -> Result<(), Failure> {
    harness::run(env!("CARGO_MANIFEST_DIR"), "colino", Data::new, Data::items)
}
