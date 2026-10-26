//! Reference tier (TESTING.md §1) for the topology report's third layer (ADR-0028): a plate
//! thinner than two voxels is one wall, measured; a gap narrower is one gap, measured;
//! neither is reported at 2.5 voxels; a dumbbell's neck is reported and its lobes are not;
//! a thin torus is one wall; a sphere has none; turning triangles finds the same places by parity; the places do not
//! depend on the thread count.

mod common;

use colino::convert::{Options, Sign, convert};
use colino::distance::Point;
use colino::mesh::Mesh;
use colino::report::{Thin, ThinKind, report};
use common::{sphere, torus};

/// Turned about x and then z, and moved off the lattice.
fn turn(p: Point) -> Point {
    let (a, b) = (0.5_f64, 0.3_f64);
    let [x, y, z] = p;
    let (y, z) = (y * a.cos() - z * a.sin(), y * a.sin() + z * a.cos());
    let (x, y) = (x * b.cos() - y * b.sin(), x * b.sin() + y * b.cos());
    [x + 0.31, y + 0.17, z - 0.23]
}

/// Axis-aligned boxes, facing out, turned by `turn`.
fn boxes(parts: &[(Point, Point)]) -> Mesh {
    let (mut vertices, mut triangles) = (Vec::new(), Vec::new());
    let faces: [[u32; 3]; 12] = [
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
    for (lo, hi) in parts {
        let base = u32::try_from(vertices.len()).unwrap();
        vertices.extend(
            (0..8).map(|i| turn([0, 1, 2].map(|k| if i >> k & 1 == 1 { hi[k] } else { lo[k] }))),
        );
        triangles.extend(faces.iter().map(|t| t.map(|i| i + base)));
    }
    Mesh::new(vertices, triangles).with_unit(Some("millimeter".into()))
}

fn plate(t: f64) -> Mesh {
    boxes(&[([-10.0, -10.0, -t / 2.0], [10.0, 10.0, t / 2.0])])
}

fn gap(g: f64) -> Mesh {
    boxes(&[
        ([-10.0 - g / 2.0, -5.0, -5.0], [-g / 2.0, 5.0, 5.0]),
        ([g / 2.0, -5.0, -5.0], [10.0 + g / 2.0, 5.0, 5.0]),
    ])
}

fn thin(m: &Mesh, o: &Options) -> Vec<Thin> {
    let c = convert(m, o).unwrap();
    let r = report(m, &c, &[]).unwrap();
    assert!(r.located);
    r.thin
}

fn options() -> Options {
    Options::new(1.0)
}

#[test]
fn a_plate_thinner_than_two_voxels_is_one_wall_its_thickness_measured() {
    for t in [0.5, 1.5] {
        let places = thin(&plate(t), &options());
        assert_eq!(places.len(), 1, "{t}: {places:?}");
        let p = places[0];
        assert_eq!(p.kind, ThinKind::Wall);
        assert!((p.least - t).abs() < 1e-9, "{t}: {}", p.least);
        // The box holds both faces, nearly to their edges.
        let span: Vec<f64> = (0..3).map(|k| p.hi[k] - p.lo[k]).collect();
        assert!(span.iter().filter(|s| **s > 15.0).count() >= 2, "{span:?}");
    }
    for t in [2.5, 3.0] {
        assert_eq!(thin(&plate(t), &options()), Vec::new(), "{t}");
    }
}

#[test]
fn a_gap_narrower_than_two_voxels_is_one_gap_its_width_measured() {
    for g in [0.5, 1.5] {
        let places = thin(&gap(g), &options());
        assert_eq!(places.len(), 1, "{g}: {places:?}");
        let p = places[0];
        assert_eq!(p.kind, ThinKind::Gap);
        assert!((p.least - g).abs() < 1e-9, "{g}: {}", p.least);
    }
    assert_eq!(thin(&gap(2.5), &options()), Vec::new());
}

/// A surface of revolution about z of the profile (r, z), pole to pole, facing out.
fn revolved(profile: &[(f64, f64)], m: u32) -> Mesh {
    let (mut vertices, mut rings) = (Vec::new(), Vec::new());
    for &(r, z) in profile {
        let start = u32::try_from(vertices.len()).unwrap();
        if r == 0.0 {
            vertices.push([0.0, 0.0, z]);
            rings.push(vec![start]);
        } else {
            for j in 0..m {
                let a = std::f64::consts::TAU * f64::from(j) / f64::from(m);
                vertices.push([r * a.cos(), r * a.sin(), z]);
            }
            rings.push((start..start + m).collect());
        }
    }
    let mut triangles = Vec::new();
    for w in rings.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        for j in 0..m as usize {
            let k = (j + 1) % m as usize;
            match (a.len(), b.len()) {
                (1, _) => triangles.push([a[0], b[k], b[j]]),
                (_, 1) => triangles.push([a[j], a[k], b[0]]),
                _ => triangles.extend([[a[j], a[k], b[k]], [a[j], b[k], b[j]]]),
            }
        }
    }
    Mesh::new(vertices, triangles).with_unit(Some("millimeter".into()))
}

#[test]
fn a_dumbbells_neck_is_reported_and_its_lobes_are_not() {
    // Lobes of radius 5 on a neck of radius 0.7 from z = −3 to 3.
    let lobe = |i: u32, sign: f64| {
        let a = std::f64::consts::PI * f64::from(i) / 24.0;
        (5.0 * a.sin(), sign * (7.0 + 5.0 * a.cos()))
    };
    let mut profile = vec![(0.0, -12.0)];
    profile.extend((1..=20).map(|i| lobe(i, -1.0)));
    profile.extend((-3..=3).map(|z| (0.7, f64::from(z))));
    profile.extend((1..=20).rev().map(|i| lobe(i, 1.0)));
    profile.push((0.0, 12.0));
    let places = thin(&revolved(&profile, 48), &options());
    let walls: Vec<&Thin> = places.iter().filter(|p| p.kind == ThinKind::Wall).collect();
    assert_eq!(walls.len(), 1, "{places:?}");
    let neck = walls[0];
    assert!(neck.least > 1.3 && neck.least < 1.4, "{}", neck.least);
    assert!(neck.lo[2] > -4.5 && neck.hi[2] < 4.5, "{neck:?}");
    assert!(neck.hi[2] - neck.lo[2] > 5.0, "{neck:?}");
}

#[test]
fn a_sphere_has_no_thin_place() {
    let m = sphere(4, 9.7, [0.3, 0.1, -0.2]).with_unit(Some("millimeter".into()));
    assert_eq!(thin(&m, &Options::new(0.7)), Vec::new());
}

#[test]
fn a_thin_torus_is_one_wall_the_width_of_its_tube() {
    let places = thin(&torus(8.0, 0.7, 64).with_unit(Some("millimeter".into())), &options());
    assert_eq!(places.len(), 1, "{places:?}");
    assert_eq!(places[0].kind, ThinKind::Wall);
    assert!(places[0].least > 1.3 && places[0].least < 1.4, "{}", places[0].least);
}

#[test]
fn turned_triangles_find_the_same_places_by_parity() {
    let parity = options().with_sign(Sign::Parity);
    for m in [plate(1.5), gap(1.5)] {
        let mut turned = m.clone();
        for t in turned.triangles.iter_mut().step_by(3) {
            t.swap(1, 2);
        }
        let (as_meant, broken) = (thin(&m, &parity), thin(&turned, &parity));
        assert_eq!(as_meant.len(), 1);
        assert_eq!(broken.len(), as_meant.len());
        // The rays cross a turned triangle with its corners in another order, so its
        // crossing's last bits may differ; everything else is the same.
        for (b, a) in broken.iter().zip(&as_meant) {
            assert_eq!((b.kind, b.lo, b.hi, b.points), (a.kind, a.lo, a.hi, a.points));
            assert!((b.least - a.least).abs() < 1e-9, "{} {}", b.least, a.least);
        }
    }
}

#[test]
fn the_places_do_not_depend_on_the_thread_count() {
    let m = gap(1.5);
    let run = |threads| {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
        pool.install(|| thin(&m, &options()))
    };
    assert_eq!(run(1), run(8));
}
