//! Reference tier (TESTING.md §1) for the topology report's first two layers (ADR-0028):
//! shapes whose Betti numbers are known, a sphere, a shell, a torus and a double torus,
//! report them, with the complement's components one more than the solid's cavities and
//! the grid's χ the input's; named points find the cavity, the outside and the wall; a wall
//! thinner than a voxel shows a change, which the third layer finds; an open input has no
//! reference.

use colino::convert::{Options, convert};
use colino::error::ReportError;
use colino::mesh::Mesh;
use colino::report::{Betti, Place, Reference, Report, report};

mod common;
use common::{sphere, torus};

/// The boundary of the union of cubes of edge `s` at `cells`, each face between a cube and
/// no cube facing out of the cube, as two triangles.
fn cubes(cells: &[[i32; 3]], s: f64) -> Mesh {
    let mut m = Mesh::new(Vec::new(), Vec::new()).with_unit(Some("millimeter".into()));
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
        assert!(r.located && r.thin.is_empty(), "{name}: {:?}", r.thin);
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
    // The third layer finds the wall, 1 thick, below 2h at h = 2 and not at h = 0.25.
    assert_eq!(fine.thin, Vec::new());
    assert_eq!(coarse.thin.len(), 1, "{:?}", coarse.thin);
    let wall = coarse.thin[0];
    assert_eq!(wall.kind, colino::report::ThinKind::Wall);
    assert!(wall.least > 0.9 && wall.least <= 1.0, "{}", wall.least);
    // At h/2, 1 voxel, the shell is whole: one solid round a cavity.
    assert!(
        matches!(wall.check, colino::report::Check::Changed { fine, .. }
            if (fine.euler, fine.solid, fine.complement) == (2, 1, 2)),
        "{:?}",
        wall.check
    );
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
    // An entry off the diagonal that is NaN is not 0 (refactoring after P5's review).
    let mut nan = convert(&m, &Options::new(1.0)).unwrap();
    nan.meta.transform[2][0] = f64::NAN;
    assert_eq!(report(&m, &nan, &[]), Err(ReportError::Transform));
    // An infinite voxel size, or an origin not finite (the re-review of P5's fixes).
    for (row, col, x) in [(0, 0, f64::INFINITY), (1, 3, f64::NAN), (2, 3, f64::NEG_INFINITY)] {
        let mut c = convert(&m, &Options::new(1.0)).unwrap();
        c.meta.transform[row][col] = x;
        assert_eq!(report(&m, &c, &[]), Err(ReportError::Transform), "[{row}][{col}] = {x}");
    }
    // Axis-aligned, but with a voxel of no size or of negative size.
    for h in [0.0, -1.0] {
        let mut c = convert(&m, &Options::new(1.0)).unwrap();
        c.meta.transform[1][1] = h;
        assert_eq!(report(&m, &c, &[("p".into(), [0.0; 3])]), Err(ReportError::Transform));
    }
    // With no point to place, the third layer reads it.
    assert_eq!(report(&m, &c, &[]), Err(ReportError::Transform));
}

/// A point is placed by the grid's own origin: a sphere at x = 100 converted on a lattice
/// starting there holds its centre in the solid and a point beyond it outside.
#[test]
fn points_are_placed_from_the_grids_origin() {
    let m = sphere(2, 10.0, [100.0, 0.0, 0.0]);
    let c = convert(&m, &Options::new(1.0).with_origin([100.0, 0.0, 0.0])).unwrap();
    let points = [("centre".into(), [100.0, 0.0, 0.0]), ("beyond".into(), [150.0, 0.0, 0.0])];
    let r = report(&m, &c, &points).unwrap();
    assert_eq!(r.regions[0].place, Place::Solid);
    assert!(matches!(r.regions[1].place, Place::Complement { outside: true, .. }));
}

/// The solid is the voxels below zero: a voxel holding -0.0, on the surface, lies outside
/// it, as ADR-0028 has the complement take φ ≥ 0.
#[test]
fn a_negative_zero_is_not_solid() {
    let m = sphere(2, 10.0, [0.0; 3]);
    let mut c = convert(&m, &Options::new(1.0)).unwrap();
    let n = c.topology.leaf_numbers().next().unwrap();
    let origin = c.topology.leaf_origin(n).unwrap();
    let leaf = c.field.leaf_mut(&c.topology, n).unwrap();
    leaf.fill(1.0);
    leaf[0] = -0.0;
    let at = origin.map(|x| f64::from(x) + 0.5);
    let r = report(&m, &c, &[("surface".into(), at)]).unwrap();
    assert!(matches!(r.regions[0].place, Place::Complement { .. }), "{:?}", r.regions[0]);
}

/// A mesh farther than 2³¹ voxels from the world's origin is refused (the re-review of
/// P5's fixes): there a ray's start off its triangle would pass the 2h it reaches, and the
/// report would find no thin place where there may be one. The lattice moves with it.
#[test]
fn a_mesh_too_far_from_the_origin_is_refused() {
    let far = 1e10;
    let m = sphere(2, 10.0, [far, 0.0, 0.0]);
    let c = convert(&m, &Options::new(1.0).with_origin([far, 0.0, 0.0])).unwrap();
    assert_eq!(report(&m, &c, &[]).map(|r| r.thin.len()), Err(ReportError::TooFar));
    // The same in voxels, at h = 2⁻¹⁰: 10⁷ is beyond 2²¹, though not beyond 2³¹.
    let (far, h) = (1e7, 1.0 / 1024.0);
    let m = sphere(2, 0.01, [far, 0.0, 0.0]);
    let c = convert(&m, &Options::new(h).with_origin([far, 0.0, 0.0])).unwrap();
    assert_eq!(report(&m, &c, &[]).map(|r| r.thin.len()), Err(ReportError::TooFar));
    let near = sphere(2, 10.0, [1e9, 0.0, 0.0]);
    let c = convert(&near, &Options::new(1.0).with_origin([1e9, 0.0, 0.0])).unwrap();
    assert!(report(&near, &c, &[]).is_ok());
}
