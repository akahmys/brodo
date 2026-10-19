//! Reference tier (TESTING.md §1) for the topology report's first two layers (ADR-0028):
//! shapes whose Betti numbers are known, a sphere, a shell, a torus and a double torus,
//! report them, with the complement's components one more than the solid's cavities and
//! the grid's χ the input's; named points find the cavity, the outside and the wall; a wall
//! thinner than a voxel shows a change; an open input has no reference.

use colino::convert::{Options, convert};
use colino::error::ReportError;
use colino::mesh::Mesh;
use colino::report::{Betti, Place, Reference, Report, report};

mod common;
use common::{sphere, torus};

/// The boundary of the union of cubes of edge `s` at `cells`, each face between a cube and
/// no cube facing out of the cube, as two triangles.
fn cubes(cells: &[[i32; 3]], s: f64) -> Mesh {
    let mut m =
        Mesh { vertices: Vec::new(), triangles: Vec::new(), unit: Some("millimeter".into()) };
    for c in cells {
        for axis in 0..3 {
            for side in [0, 1] {
                let mut n = *c;
                n[axis] += 2 * side - 1;
                if cells.contains(&n) {
                    continue;
                }
                let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
                let corner = |a: i32, b: i32| {
                    let mut p = c.map(f64::from);
                    p[axis] += f64::from(side);
                    p[u] += f64::from(a);
                    p[v] += f64::from(b);
                    p.map(|x| x * s)
                };
                let base = u32::try_from(m.vertices.len()).unwrap();
                m.vertices.extend([corner(0, 0), corner(1, 0), corner(1, 1), corner(0, 1)]);
                // u × v points along +axis: counter-clockwise from outside on the high side.
                let quad = if side == 1 { [0, 1, 2, 3] } else { [0, 3, 2, 1] };
                let q = quad.map(|i| base + i);
                m.triangles.extend([[q[0], q[1], q[2]], [q[0], q[2], q[3]]]);
            }
        }
    }
    m
}

/// A 5 × 3 × 1 slab of cubes with two holes through it: a double torus.
fn double_torus(s: f64) -> Mesh {
    let mut cells = Vec::new();
    for y in 0..3 {
        for x in 0..5 {
            if y != 1 || x % 2 == 0 {
                cells.push([x, y, 0]);
            }
        }
    }
    cubes(&cells, s)
}

/// A sphere of radius 20 holding a cavity of radius `inner`: the inner sphere faces in.
fn shell(inner: f64) -> Mesh {
    let mut m = sphere(4, 20.0, [0.0; 3]);
    let hole = sphere(4, inner, [0.0; 3]);
    let base = u32::try_from(m.vertices.len()).unwrap();
    m.vertices.extend(hole.vertices);
    m.triangles.extend(hole.triangles.iter().map(|[a, b, c]| [a + base, c + base, b + base]));
    m
}

fn reported(m: &Mesh, h: f64, points: &[(&str, [f64; 3])]) -> Report {
    let c = convert(m, &Options::new(h)).unwrap();
    let named: Vec<_> = points.iter().map(|(n, p)| (n.to_string(), *p)).collect();
    report(m, &c, &named).unwrap()
}

fn betti(b: [i64; 3]) -> Betti {
    Betti { components: b[0], tunnels: b[1], cavities: b[2] }
}

#[test]
fn known_shapes_report_their_betti_numbers() {
    for (name, mesh, h, solid, chi) in [
        ("sphere", sphere(4, 20.0, [0.0; 3]), 1.0, [1, 0, 0], 1),
        ("shell", shell(12.0), 1.0, [1, 0, 1], 2),
        ("torus", torus(20.0, 7.0, 48), 1.0, [1, 1, 0], 0),
        ("double torus", double_torus(8.0), 1.0, [1, 2, 0], -1),
    ] {
        let r = reported(&mesh, h, &[]);
        assert_eq!(r.solid, betti(solid), "{name}");
        assert_eq!(r.euler, chi, "{name}");
        // Counted, the complement's components are the outside and each cavity.
        assert_eq!(r.complement.components, 1 + r.solid.cavities, "{name}");
        assert_eq!(r.complement, betti([1 + solid[2], solid[1], solid[0] - 1]), "{name}");
        assert_eq!(
            r.reference,
            Reference::Closed { surface_euler: 2 * chi, solid_euler: chi },
            "{name}"
        );
        assert_eq!(r.euler_change, Some(0), "{name}");
        assert!(!r.located, "{name}");
    }
}

#[test]
fn named_points_find_the_cavity_the_wall_and_the_outside() {
    let r = reported(
        &shell(12.0),
        1.0,
        &[
            ("cavity", [0.5, 0.5, 0.5]),
            ("wall", [16.0, 0.5, 0.5]),
            ("near", [30.0, 0.5, 0.5]),
            ("far", [1e6, -3e5, 2e5]),
            ("beyond", [1e12, 0.0, 0.0]),
            ("also the cavity", [-5.0, 3.0, 2.0]),
        ],
    );
    let places: Vec<Place> = r.regions.iter().map(|g| g.place).collect();
    let Place::Complement { component: cavity, outside: false } = places[0] else {
        panic!("{places:?}")
    };
    assert_eq!(places[1], Place::Solid);
    let Place::Complement { component: out, outside: true } = places[2] else {
        panic!("{places:?}")
    };
    assert_ne!(cavity, out);
    for p in &places[3..5] {
        assert_eq!(*p, Place::Complement { component: out, outside: true });
    }
    assert_eq!(places[5], places[0]);
    assert_eq!(r.together, vec![(0, 5), (2, 3), (2, 4), (3, 4)]);
    assert_eq!(r.apart, vec![(0, 2), (0, 3), (0, 4), (2, 5), (3, 5), (4, 5)]);
    assert_eq!(r.regions[1].name, "wall");
}

#[test]
fn a_wall_thinner_than_a_voxel_shows_a_change() {
    let fine = reported(&shell(19.0), 0.25, &[]);
    assert_eq!(fine.euler_change, Some(0), "{:?}", fine.solid);
    // At h = 2 the wall falls between voxel centres in places: its pieces, and the holes
    // left through it, as measured (2026-10-20).
    let coarse = reported(&shell(19.0), 2.0, &[]);
    assert_eq!((coarse.solid, coarse.euler), (betti([13, 107, 0]), -94));
    assert_eq!(coarse.euler_change, Some(-96));
}

#[test]
fn a_sphere_between_voxel_centres_leaves_no_solid() {
    // Radius 0.3 about a voxel corner: every voxel centre lies outside it.
    let r = reported(&sphere(2, 0.3, [0.0; 3]), 1.0, &[]);
    assert_eq!((r.solid, r.euler, r.euler_change), (betti([0, 0, 0]), 0, Some(-1)));
    assert_eq!(r.complement, betti([1, 0, 1]));
}

#[test]
fn an_open_input_has_no_reference() {
    let mut m = sphere(4, 20.0, [0.0; 3]);
    m.triangles.pop();
    let r = reported(&m, 1.0, &[]);
    let Reference::None { surface } = r.reference else { panic!("{:?}", r.reference) };
    assert_eq!(surface.open_edges, 3);
    assert_eq!(r.euler_change, None);
}

#[test]
fn a_transform_not_axis_aligned_is_refused() {
    let m = sphere(2, 10.0, [0.0; 3]);
    let mut c = convert(&m, &Options::new(1.0)).unwrap();
    c.meta.transform[0][1] = 0.5;
    assert_eq!(report(&m, &c, &[("p".into(), [0.0; 3])]), Err(ReportError::Transform));
    // With no point to place, the transform is never read.
    assert!(report(&m, &c, &[]).is_ok());
}
