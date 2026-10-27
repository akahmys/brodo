//! Reference tier (TESTING.md §1) for the topology report's third layer (ADR-0028): a plate
//! thinner than two voxels is one wall, measured; a gap narrower is one gap, measured;
//! each is checked at h/2, the plate and gap at 0.5 found changed and at 1.5 the same, and a
//! place whose box is too large left unchecked; neither is reported at 2.5 voxels; a dumbbell's neck is reported and its lobes are not;
//! a thin torus is one wall; a sphere has none; turning triangles finds the same places by parity; the places do not
//! depend on the thread count.

mod common;

use colino::convert::{Options, Sign, convert};
use colino::distance::Point;
use colino::mesh::Mesh;
use colino::report::{Check, Invariants, Thin, ThinKind, report};
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

/// Invariants as a user outside the crate reads them, compared field by field.
fn invariants(euler: i64, solid: i64, complement: i64) -> Invariants {
    let c = convert(&plate(1.5), &options()).unwrap();
    let r = report(&plate(1.5), &c, &[]).unwrap();
    let Check::Same(mut i) = r.thin[0].check else { panic!("{:?}", r.thin[0].check) };
    (i.euler, i.solid, i.complement) = (euler, solid, complement);
    i
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
        // At 0.5 the conversion opens holes in the plate; at h/2 it is whole. At 1.5 both
        // see one plate.
        let plate = invariants(1, 1, 1);
        if t < 1.0 {
            assert!(
                matches!(p.check, Check::Changed { coarse, fine } if coarse.euler < 1 && fine == plate),
                "{:?}",
                p.check
            );
        } else {
            assert_eq!(p.check, Check::Same(plate));
        }
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
        // At 0.5 the conversion closes the gap, joining the boxes; at 1.5 they stay apart.
        if g < 1.0 {
            assert!(
                matches!(p.check, Check::Changed { coarse, .. } if coarse.solid == 1),
                "{:?}",
                p.check
            );
        } else {
            assert_eq!(p.check, Check::Same(invariants(2, 2, 1)));
        }
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
fn a_place_whose_box_is_too_large_at_h_over_2_is_unchecked() {
    // A rod of radius 0.25, 370 long, turned from z onto (1, 1, 1): few points on it, but
    // its box holds about 216³ voxels at h, more than 2²⁶ at h/2.
    let rod = revolved(&[(0.0, 0.0), (0.25, 0.0), (0.25, 370.0), (0.0, 370.0)], 8);
    let (e3, e1) = ([1.0 / 3.0_f64.sqrt(); 3], [1.0, -1.0, 0.0].map(|x| x / 2.0_f64.sqrt()));
    let e2 = [1.0, 1.0, -2.0].map(|x| x / 6.0_f64.sqrt());
    let vertices = rod
        .vertices
        .iter()
        .map(|p| [0, 1, 2].map(|k| 0.3 + p[0] * e1[k] + p[1] * e2[k] + p[2] * e3[k]))
        .collect();
    let m = Mesh::new(vertices, rod.triangles).with_unit(Some("millimeter".into()));
    let places = thin(&m, &options());
    assert_eq!(places.len(), 1, "{places:?}");
    assert_eq!(places[0].kind, ThinKind::Wall);
    assert_eq!(places[0].check, Check::Unchecked);
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

/// The ray's start off its own triangle follows the coordinates' size (N3 of P5's review):
/// a cube far from the origin has no thin place, where a start of reach·10⁻⁹, lost to
/// rounding there, crossed the ray's own face and measured walls as thin as rounding.
#[test]
fn a_cube_far_from_the_origin_has_no_thin_place() {
    let far = 1e9;
    let m = boxes(&[([far, 0.0, 0.0], [far + 10.0, 10.0, 10.0])]);
    assert_eq!(thin(&m, &options()), Vec::new());
}

/// At h = 0.5 on a lattice moved off the origin (W3, W4 of P5's review): two plates 0.6
/// thick and 1.5 apart are two walls, their thin points farther apart than 2h, each
/// measured, sampled h/2 apart over both faces, and checked where it lies.
#[test]
fn two_plates_at_half_a_voxel_are_two_walls_checked_where_they_lie() {
    let (t, apart) = (0.6, 1.5);
    let m = boxes(&[
        ([-3.0, -3.0, 0.0], [3.0, 3.0, t]),
        ([-3.0, -3.0, t + apart], [3.0, 3.0, 2.0 * t + apart]),
    ]);
    let o = Options::new(0.5).with_origin([7.3, -5.2, 3.1]);
    let places = thin(&m, &o);
    assert_eq!(places.len(), 2, "{places:?}");
    for p in &places {
        assert_eq!(p.kind, ThinKind::Wall);
        assert!((p.least - t).abs() < 1e-9, "{}", p.least);
        // Two faces of 36 each, a point every h/2 = 0.25: some 1150 points.
        assert!(p.points > 1000, "{}", p.points);
        assert!(matches!(p.check, Check::Same(i) if i.solid >= 1), "{:?}", p.check);
    }
}
