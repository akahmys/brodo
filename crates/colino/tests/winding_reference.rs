//! Reference tier (TESTING.md §1) for the winding number (ADR-0027): known solid angles;
//! 1 inside and 0 outside closed surfaces; the far-field expansion's error falling as the
//! fifth power of distance, as a second-order expansion's should; and, on soups open,
//! overlapping and badly oriented, the approximation with β = 2 on the same side of 1/2 as
//! the exact sum wherever that is not close to 1/2.

use std::f64::consts::PI;

use colino::distance::Point;
use colino::mesh::Mesh;
use colino::winding::{Accuracy, Winding, solid_angle};
use proptest::prelude::*;

const BETA: Accuracy = Accuracy::Approximate { beta: 2.0 };

fn octahedron() -> Mesh {
    let vertices = vec![
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
    ];
    let triangles = vec![
        [0, 2, 4],
        [2, 1, 4],
        [1, 3, 4],
        [3, 0, 4],
        [2, 0, 5],
        [1, 2, 5],
        [3, 1, 5],
        [0, 3, 5],
    ];
    Mesh { vertices, triangles, unit: None }
}

/// The octahedron with each face split in four, `levels` times, its vertices pushed out to
/// the unit sphere: a closed, outward surface.
fn sphere(levels: u32) -> Mesh {
    let mut m = octahedron();
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
    m
}

#[test]
fn an_octant_subtends_an_eighth_of_the_sphere() {
    let o = solid_angle([0.0; 3], [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
    assert!((o - PI / 2.0).abs() < 1e-15, "{o}");
    let reversed = solid_angle([0.0; 3], [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]]);
    assert!((reversed + PI / 2.0).abs() < 1e-15, "{reversed}");
}

#[test]
fn closed_surfaces_wind_once_inside_and_not_outside() {
    for mesh in [octahedron(), sphere(3)] {
        let w = Winding::new(&mesh).unwrap();
        for (q, inside) in [
            ([0.0, 0.0, 0.0], true),
            ([0.1, -0.2, 0.3], true),
            ([2.0, 0.0, 0.0], false),
            ([0.5, 0.5, 0.9], false),
            ([-3.0, 4.0, 1.0], false),
        ] {
            let want = if inside { 1.0 } else { 0.0 };
            assert!((w.number(q, Accuracy::Exact) - want).abs() < 1e-12, "{q:?}");
            assert!((w.number(q, BETA) - want).abs() < 0.01, "{q:?}: {}", w.number(q, BETA));
        }
    }
}

#[test]
fn the_expansion_error_falls_as_the_fifth_power_of_distance() {
    // A cluster of 40 triangles in the unit box, taken whole at the root by β = 0.
    let mut g = 12345_u64;
    let mut r = || {
        g = g.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (g >> 11) as f64 / (1_u64 << 53) as f64
    };
    let vertices: Vec<Point> = (0..120).map(|_| [r(), r(), r()]).collect();
    let triangles = (0..40).map(|i| [3 * i, 3 * i + 1, 3 * i + 2]).collect();
    let w = Winding::new(&Mesh { vertices, triangles, unit: None }).unwrap();
    let error = |d: f64| {
        let q = [0.6, -0.48, 0.64].map(|c| 0.5 + c * d);
        (w.number(q, Accuracy::Approximate { beta: 0.0 }) - w.number(q, Accuracy::Exact)).abs()
    };
    for d in [4.0, 8.0, 16.0] {
        let ratio = error(d) / error(2.0 * d);
        assert!((28.0..36.0).contains(&ratio), "from {d}: {ratio}");
    }
}

fn coordinate() -> impl Strategy<Value = f64> {
    -2.0..2.0_f64
}

fn point() -> impl Strategy<Value = Point> {
    [coordinate(), coordinate(), coordinate()]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn the_approximation_falls_on_the_exact_sums_side(
        soup in prop::collection::vec([point(), point(), point()], 1..80),
        closed in any::<bool>(),
        flips in prop::collection::vec(any::<bool>(), 0..20),
        // Near the shapes, inside the sphere often; and far, where nodes are taken whole.
        queries in prop::collection::vec(prop_oneof![[-1.2..1.2_f64, -1.2..1.2, -1.2..1.2], [-10.0..10.0_f64, -10.0..10.0, -10.0..10.0]], 20),
    ) {
        // A soup of triangles, or the subdivided sphere with some faces turned over.
        let mesh = if closed {
            let mut m = sphere(2);
            for (t, flip) in m.triangles.iter_mut().zip(&flips) {
                if *flip {
                    t.swap(1, 2);
                }
            }
            m
        } else {
            let vertices: Vec<Point> = soup.iter().flatten().copied().collect();
            let triangles = (0..soup.len()).map(|i| u32::try_from(i).unwrap()).map(|i| [3 * i, 3 * i + 1, 3 * i + 2]).collect();
            Mesh { vertices, triangles, unit: None }
        };
        let w = Winding::new(&mesh).unwrap();
        for q in queries {
            let exact = w.number(q, Accuracy::Exact);
            let approx = w.number(q, BETA);
            let far = w.number(q, Accuracy::Approximate { beta: 1e9 });
            prop_assert!((far - exact).abs() < 1e-9, "β huge: {} against {}", far, exact);
            if (exact - 0.5).abs() >= 0.05 {
                prop_assert_eq!(approx >= 0.5, exact >= 0.5, "{} against {} at {:?}", approx, exact, q);
            }
        }
    }
}
