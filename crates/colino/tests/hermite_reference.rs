//! Reference tier (TESTING.md §1) for Hermite data (ADR-0030's acceptance tests): a cube
//! turned about two axes, each face a group, gives every crossing where its face's plane
//! meets the edge, to `f32` rounding, with its face's normal and group; a closed mesh gives
//! the same bytes at one and at eight threads; a torus's crossed edges are all hit; every
//! set keeps to brodo's format, whose check covers each crossed edge's owner being a leaf
//! point, at bands of 1 to 3; an open mesh counts its edges with no triangle hit; of the
//! hits on an edge, the one nearest its inside end is kept, whichever end that is; a
//! triangle with no area never gives an unhit edge its data; a normal perpendicular to its
//! edge keeps its triangle's side; a face turned in, refused by the winding number beyond
//! the band, gives by parity the cube's set as meant.

mod boxes;
mod common;

use brodo::cache::to_bytes;
use brodo::error::HermiteError;
use brodo::meta::{GridMeta, SDF_KEY};
use brodo::topology::Location;
use colino::convert::{Options, convert};
use colino::distance::Point;
use colino::hermite::{Produced, hermite};
use colino::mesh::Mesh;
use colino::winding::Accuracy;

/// How far a stored fraction may be from the exact one: `f32` rounding of a value below 1,
/// and the reference's own `f64` arithmetic.
const FRACTION_TOLERANCE: f64 = 1.5e-7;
/// How far a stored normal may be from its face's: `f32` rounding of a unit vector.
const NORMAL_TOLERANCE: f64 = 1e-6;
/// Crossings this near a cube's edge, in voxels, may be on either face and are not compared.
const EDGE_MARGIN: f64 = 1e-6;

fn rotate(p: Point, a: f64, b: f64) -> Point {
    let (sa, ca, sb, cb) = (a.sin(), a.cos(), b.sin(), b.cos());
    // About x by a, then about y by b.
    let q = [p[0], ca * p[1] - sa * p[2], sa * p[1] + ca * p[2]];
    [cb * q[0] + sb * q[2], q[1], -sb * q[0] + cb * q[2]]
}

/// A cube of half-side `r` about `centre`, turned about x by `a` and y by `b`; each face its
/// own group, named for its outward axis, facing out.
fn turned_cube(r: f64, centre: Point, a: f64, b: f64) -> Mesh {
    let mut vertices = Vec::new();
    for i in 0..8 {
        let corner = [0, 1, 2].map(|k| if i >> k & 1 == 1 { r } else { -r });
        let p = rotate(corner, a, b);
        vertices.push([0, 1, 2].map(|k| centre[k] + p[k]));
    }
    // Faces as quads, counter-clockwise seen from outside.
    let quads: [[u32; 4]; 6] =
        [[0, 4, 6, 2], [1, 3, 7, 5], [0, 1, 5, 4], [2, 6, 7, 3], [0, 2, 3, 1], [4, 5, 7, 6]];
    let mut triangles = Vec::new();
    let mut groups = Vec::new();
    for (f, [p, q, s, t]) in (1_u32..).zip(quads) {
        triangles.extend([[p, q, s], [p, s, t]]);
        groups.extend([f, f]);
    }
    let group_names =
        ["", "-x", "+x", "-y", "+y", "-z", "+z"].iter().map(|s| (*s).to_owned()).collect();
    Mesh::new(vertices, triangles)
        .with_unit(Some("millimeter".into()))
        .with_groups(groups, group_names)
}

fn made(m: &Mesh, o: &Options) -> Produced {
    let c = convert(m, o).unwrap();
    hermite(m, &c).unwrap()
}

/// The turned cube's faces in exact arithmetic, as far as `f64` goes.
struct Cube {
    r: f64,
    centre: Point,
    a: f64,
    b: f64,
    /// Each face's outward normal and offset along it, in the order of `turned_cube`.
    faces: Vec<(Point, f64)>,
}

impl Cube {
    fn new(r: f64, centre: Point, a: f64, b: f64) -> Cube {
        let axes = [
            [-1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, -1.0],
            [0.0, 0.0, 1.0],
        ];
        let faces = axes
            .iter()
            .map(|n| {
                let n = rotate(*n, a, b);
                (n, r + (0..3).map(|k| n[k] * centre[k]).sum::<f64>())
            })
            .collect();
        Cube { r, centre, a, b, faces }
    }

    /// A world point in the cube's frame before it was turned.
    fn local(&self, p: Point) -> Point {
        let q = [0, 1, 2].map(|k| p[k] - self.centre[k]);
        let (sa, ca, sb, cb) = (self.a.sin(), self.a.cos(), self.b.sin(), self.b.cos());
        let u = [cb * q[0] - sb * q[2], q[1], sb * q[0] + cb * q[2]];
        [u[0], ca * u[1] + sa * u[2], -sa * u[1] + ca * u[2]]
    }

    /// The face the edge of length `h` from `start` along `axis` crosses, its fraction, and
    /// whether the crossing is too near the cube's edge to tell the face.
    fn crossing(&self, start: Point, axis: usize, h: f64) -> Option<(usize, f64, bool)> {
        let mut best = None;
        for (f, (n, d)) in self.faces.iter().enumerate() {
            if n[axis].abs() < 1e-12 {
                continue;
            }
            let s = (d - (0..3).map(|k| n[k] * start[k]).sum::<f64>()) / (n[axis] * h);
            if !(0.0..=1.0).contains(&s) {
                continue;
            }
            let mut p = start;
            p[axis] += h * s;
            let l = self.local(p);
            if l.iter().all(|x| x.abs() <= self.r + 1e-9) {
                let edges = l.iter().filter(|x| (x.abs() - self.r).abs() < EDGE_MARGIN).count();
                best = Some((f, s, edges > 1));
            }
        }
        best
    }
}

#[test]
fn a_turned_cube_crosses_each_edge_where_its_faces_plane_does() {
    let cube = Cube::new(6.3, [0.37, -0.21, 0.13], 0.4, 0.7);
    let m = turned_cube(cube.r, cube.centre, cube.a, cube.b);
    let o = Options::new(0.5).with_accuracy(Accuracy::Exact);
    let p = hermite(&m, &convert(&m, &o).unwrap());
    // Brodo's check refuses a normal pointing in, among the rest.
    assert!(p.is_ok(), "{p:?}");
    let p = p.unwrap();
    assert_eq!(p.unhit, 0);
    let (set, t) = (&p.set, p.set.topology());
    let (mut compared, mut skipped) = (0, 0);
    for n in t.leaf_numbers() {
        let voxels: Vec<_> = t.leaf_voxels(n).unwrap().collect();
        for (axis, c) in set.axes().iter().enumerate() {
            let (fractions, normals) =
                (c.fraction.leaf(t, n).unwrap(), c.normal.leaf(t, n).unwrap());
            let groups = c.group.leaf(t, n).unwrap();
            for (i, v) in voxels.iter().enumerate() {
                if fractions[i] < 0.0 {
                    continue;
                }
                let found = cube.crossing(v.map(|x| 0.5 * f64::from(x)), axis, 0.5);
                let Some((f, s, near_edge)) = found else { panic!("no face crosses {v:?} {axis}") };
                if near_edge {
                    skipped += 1;
                    continue;
                }
                compared += 1;
                assert!((f64::from(fractions[i]) - s).abs() <= FRACTION_TOLERANCE, "{v:?} {axis}");
                assert_eq!(groups[i], u32::try_from(f).unwrap() + 1, "{v:?} {axis}");
                let inside = set.signs().value_at(t, *v).unwrap() == 1;
                let want = cube.faces[f].0;
                let inward = if inside { want[axis] < 0.0 } else { want[axis] > 0.0 };
                let out = if inward { want.map(|x| -x) } else { want };
                for k in 0..3 {
                    assert!((f64::from(normals[i][k]) - out[k]).abs() <= NORMAL_TOLERANCE);
                }
            }
        }
    }
    assert!(compared > 1000 && skipped < compared / 50, "{compared} compared, {skipped} skipped");
}

#[test]
fn the_bytes_do_not_depend_on_the_thread_count() {
    let m = common::torus(20.0, 7.0, 32);
    let run = |threads| {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
        let p = pool.install(|| made(&m, &Options::new(1.0)));
        let meta = GridMeta::new("torus", [1.0; 3], "millimeter").unwrap();
        (to_bytes(&p.set.entries("torus", &meta).unwrap()).unwrap(), p.unhit)
    };
    assert_eq!(run(1), run(8));
}

#[test]
fn every_crossed_edge_of_a_torus_is_hit() {
    let m = common::torus(20.0, 7.0, 48);
    let p = made(&m, &Options::new(1.0).with_accuracy(Accuracy::Exact));
    assert_eq!(p.unhit, 0);
    let t = p.set.topology();
    let crossed: usize = t
        .leaf_numbers()
        .flat_map(|n| p.set.axes().iter().map(move |c| (n, c)))
        .map(|(n, c)| c.fraction.leaf(t, n).unwrap().iter().filter(|f| **f >= 0.0).count())
        .sum();
    assert!(crossed > 5_000, "{crossed} crossed edges");
    // A lattice point on the mesh is inside, and an edge it is the inside end of crosses
    // exactly there.
    let bvh = colino::bvh::Bvh::new(&m).unwrap();
    let on_mesh = |v: [i32; 3]| bvh.nearest(v.map(f64::from)).is_some_and(|(d, _)| d == 0.0);
    let (mut on, mut upper) = (0, 0);
    for n in t.leaf_numbers() {
        let voxels: Vec<_> = t.leaf_voxels(n).unwrap().collect();
        for (i, v) in voxels.iter().enumerate() {
            if !on_mesh(*v) {
                continue;
            }
            on += 1;
            assert_eq!(p.set.signs().value_at(t, *v).unwrap(), 1, "{v:?}");
            for c in p.set.axes() {
                let f = c.fraction.leaf(t, n).unwrap()[i];
                assert!(f <= 0.0, "{v:?}: {f}");
            }
            // The edge into it from an outside point below crosses exactly at its upper end
            // (W7 of P5's review).
            for (k, c) in p.set.axes().iter().enumerate() {
                let mut u = *v;
                u[k] -= 1;
                let Location::Leaf(m) = t.locate(u) else { continue };
                if p.set.signs().value_at(t, u).unwrap() != 0 {
                    continue;
                }
                let at = t.leaf_voxels(m).unwrap().position(|w| w == u).unwrap();
                let f = c.fraction.leaf(t, m).unwrap()[at];
                assert_eq!(f.to_bits(), 1.0_f32.to_bits(), "{u:?} along {k}: {f}");
                upper += 1;
            }
        }
    }
    assert!(on > 0, "no lattice point on the mesh");
    assert!(upper > 0, "no edge up to a lattice point on the mesh");
}

#[test]
fn every_band_keeps_crossed_edges_owned_by_leaves() {
    // Hermite::new refuses a set with a crossed edge whose owner is no leaf point.
    for band in 1..=3 {
        for m in [common::sphere(3, 9.7, [0.3, 0.1, -0.2]), common::torus(12.0, 4.1, 32)] {
            let o = Options::new(0.7).with_band(band);
            let c = convert(&m, &o).unwrap();
            assert!(hermite(&m, &c).is_ok(), "band {band}");
        }
    }
}

#[test]
fn an_open_mesh_counts_its_edges_with_no_triangle_hit() {
    let closed = common::sphere(4, 20.0, [0.0; 3]);
    let mut holed = closed.clone();
    let middle_z =
        |t: &[u32; 3]| t.iter().map(|i| closed.vertices[*i as usize][2]).sum::<f64>() / 3.0;
    holed.triangles.retain(|t| middle_z(t) <= 18.0);
    let o = Options::new(1.0);
    assert_eq!(made(&closed, &o).unhit, 0);
    assert!(made(&holed, &o).unhit > 0);
}

/// Boxes appended to one mesh, each its own group in order.
fn boxes(parts: &[(Point, Point, bool)]) -> Mesh {
    let mut m = Mesh::default().with_unit(Some("millimeter".into()));
    m.group_names.push(String::new());
    for (g, (lo, hi, inward)) in (1_u32..).zip(parts) {
        let (v, t) = boxes::boxed(*lo, *hi, *inward);
        let base = u32::try_from(m.vertices.len()).unwrap();
        m.vertices.extend(v);
        m.triangles.extend(t.iter().map(|t| t.map(|i| i + base)));
        m.groups.extend(std::iter::repeat_n(g, t.len()));
        m.group_names.push(format!("box {g}"));
    }
    m
}

#[test]
fn the_crossing_nearest_the_inside_end_and_the_lowest_triangle_are_kept() {
    // A wall from 4.4 to 5 round a cavity holding a solid to 4.3, and the solid again, a
    // copy of the same faces after it. The edge from 4.25 to 5.25 along x crosses at 4.3,
    // twice, 4.4 and 5: the crossing nearest its inside end is 4.3, from the first solid.
    let m = boxes(&[
        ([-5.0; 3], [5.0; 3], false),
        ([-4.4; 3], [4.4; 3], true),
        ([-4.3; 3], [4.3; 3], false),
        ([-4.3; 3], [4.3; 3], false),
    ]);
    let o = Options::new(1.0).with_origin([0.25; 3]);
    let p = made(&m, &o);
    let t = p.set.topology();
    let v = [4, 0, 0];
    let n = match t.locate(v) {
        brodo::topology::Location::Leaf(n) => n,
        other @ (brodo::topology::Location::Tile(..) | brodo::topology::Location::Background) => {
            panic!("{other:?}")
        }
    };
    let i = t.leaf_voxels(n).unwrap().position(|w| w == v).unwrap();
    let x = &p.set.axes()[0];
    assert_eq!(p.set.signs().value_at(t, v).unwrap(), 1);
    assert!(
        (f64::from(x.fraction.leaf(t, n).unwrap()[i]) - 0.05).abs() <= FRACTION_TOLERANCE * 10.0
    );
    assert_eq!(x.group.leaf(t, n).unwrap()[i], 3);
    assert_eq!(
        x.normal.leaf(t, n).unwrap()[i].map(f32::to_bits),
        [1.0_f32, 0.0, 0.0].map(f32::to_bits)
    );
}

#[test]
fn an_unhit_edge_takes_the_triangle_with_area_nearest_it() {
    use colino::intersect::{crossing, nearest};
    let closed = common::sphere(3, 20.0, [0.0; 3]);
    let mut m = closed.clone();
    let middle_z =
        |t: &[u32; 3]| t.iter().map(|i| closed.vertices[*i as usize][2]).sum::<f64>() / 3.0;
    m.triangles.retain(|t| middle_z(t) <= 17.0);
    // Each triangle its own group, so the group names the triangle.
    m.groups = (1..=u32::try_from(m.triangles.len()).unwrap()).collect();
    m.group_names = std::iter::once(String::new())
        .chain((1..=m.triangles.len()).map(|i| format!("t{i}")))
        .collect();
    let p = made(&m, &Options::new(1.0));
    let corners: Vec<[Point; 3]> =
        m.triangles.iter().map(|t| t.map(|i| m.vertices[i as usize])).collect();
    let t = p.set.topology();
    let mut checked = 0;
    for n in t.leaf_numbers() {
        let voxels: Vec<_> = t.leaf_voxels(n).unwrap().collect();
        for (axis, c) in p.set.axes().iter().enumerate() {
            for (i, v) in voxels.iter().enumerate() {
                let f = c.fraction.leaf(t, n).unwrap()[i];
                if f < 0.0 {
                    continue;
                }
                let a = v.map(f64::from);
                let mut b = a;
                b[axis] += 1.0;
                if corners.iter().any(|tri| crossing(a, b, *tri).is_some()) {
                    continue;
                }
                let (mut best, mut at) = (f64::INFINITY, (0.0, 0));
                for (k, tri) in corners.iter().enumerate() {
                    let (s, d) = nearest(a, b, *tri);
                    if d < best {
                        (best, at) = (d, (s, k));
                    }
                }
                checked += 1;
                assert_eq!(c.group.leaf(t, n).unwrap()[i], u32::try_from(at.1).unwrap() + 1);
                assert!((f64::from(f) - at.0).abs() <= FRACTION_TOLERANCE, "{v:?} {axis}");
            }
        }
    }
    assert_eq!(u64::try_from(checked).unwrap(), p.unhit);
    assert!(checked > 10, "{checked} unhit edges");
}

#[test]
fn a_transform_not_axis_aligned_is_refused() {
    let m = common::sphere(1, 5.0, [0.0; 3]);
    let o = Options::new(1.0);
    let mut c = convert(&m, &o).unwrap();
    c.meta.transform[0][1] = 0.5;
    assert!(matches!(hermite(&m, &c), Err(colino::error::HermiteError::Transform)));
}

#[test]
fn a_triangle_turned_in_still_gives_normals_pointing_out() {
    // One triangle of a sphere turned over, in a group of its own; the winding number
    // barely changes, so the signs do not.
    let mut m = common::sphere(3, 20.0, [0.3, 0.1, -0.2]);
    m.triangles[0].swap(1, 2);
    m.groups = vec![0; m.triangles.len()];
    m.groups[0] = 1;
    m.group_names = vec![String::new(), "turned".into()];
    let o = Options::new(0.5).with_accuracy(Accuracy::Exact);
    let p = hermite(&m, &convert(&m, &o).unwrap());
    assert!(p.is_ok(), "{p:?}");
    let p = p.unwrap();
    let t = p.set.topology();
    let mut turned = 0;
    for n in t.leaf_numbers() {
        let voxels: Vec<_> = t.leaf_voxels(n).unwrap().collect();
        for (axis, c) in p.set.axes().iter().enumerate() {
            for (i, v) in voxels.iter().enumerate() {
                if c.group.leaf(t, n).unwrap()[i] != 1 || c.fraction.leaf(t, n).unwrap()[i] < 0.0 {
                    continue;
                }
                let inside = p.set.signs().value_at(t, *v).unwrap() == 1;
                let along = c.normal.leaf(t, n).unwrap()[i][axis];
                assert!(if inside { along >= 0.0 } else { along <= 0.0 }, "{v:?} {axis}");
                turned += 1;
            }
        }
    }
    assert!(turned > 10, "{turned} crossings on the turned triangle");
}

#[test]
fn a_face_turned_in_is_refused_beyond_the_band_and_counted_within_it() {
    // The winding number's 1/2 lies 8 voxels inside the turned face (2026-10-22).
    let cube = Cube::new(6.3, [0.37, -0.21, 0.13], 0.4, 0.7);
    let mut m = turned_cube(cube.r, cube.centre, cube.a, cube.b);
    for t in m.triangles.iter_mut().skip(2).take(2) {
        t.swap(1, 2);
    }
    let run = |band| {
        let o = Options::new(0.5).with_band(band).with_accuracy(Accuracy::Exact);
        hermite(&m, &convert(&m, &o).unwrap())
    };
    assert!(matches!(run(3), Err(colino::error::HermiteError::SignOutsideBand(_))));
    let wide = run(6);
    assert!(wide.as_ref().is_ok_and(|p| p.unhit > 0), "{:?}", wide.map(|p| p.unhit));
}

#[test]
fn a_face_turned_in_gives_the_cubes_hermite_data_by_parity() {
    // The case above, refused by the winding number at a band of 3 (ADR-0030), gives by
    // parity the set of the cube as meant, to the byte, every crossed edge hit (ADR-0027).
    let cube = Cube::new(6.3, [0.37, -0.21, 0.13], 0.4, 0.7);
    let as_meant = turned_cube(cube.r, cube.centre, cube.a, cube.b);
    let mut m = as_meant.clone();
    for t in m.triangles.iter_mut().skip(2).take(2) {
        t.swap(1, 2);
    }
    let o = Options::new(0.5).with_sign(colino::convert::Sign::Parity);
    let meta = GridMeta::new("cube", [0.5; 3], "millimeter").unwrap();
    let set = |m: &Mesh| {
        let p = hermite(m, &convert(m, &o).unwrap()).unwrap();
        (to_bytes(&p.set.entries("cube", &meta).unwrap()).unwrap(), p.unhit)
    };
    let (broken, unhit) = set(&m);
    assert_eq!(unhit, 0);
    assert!(broken == set(&as_meant).0);
}

/// The value index of point `v` in its leaf, which must exist.
fn place(t: &brodo::topology::Topology, v: [i32; 3]) -> (brodo::topology::LeafNumber, usize) {
    let n = match t.locate(v) {
        brodo::topology::Location::Leaf(n) => n,
        other @ (brodo::topology::Location::Tile(..) | brodo::topology::Location::Background) => {
            panic!("{v:?}: {other:?}")
        }
    };
    (n, t.leaf_voxels(n).unwrap().position(|w| w == v).unwrap())
}

#[test]
fn a_triangle_with_no_area_never_gives_an_unhit_edge_its_data() {
    use colino::intersect::{crossing, nearest};
    // A sphere holed at the top, a triangle with no area across the hole, nearer some of
    // its unhit edges than any triangle with area. With it taken, its zero normal would
    // break the format and the set be refused.
    let closed = common::sphere(3, 20.0, [0.0; 3]);
    let mut m = closed.clone();
    let middle_z =
        |t: &[u32; 3]| t.iter().map(|i| closed.vertices[*i as usize][2]).sum::<f64>() / 3.0;
    m.triangles.retain(|t| middle_z(t) <= 17.0);
    let flat = [[-9.0, 0.4, 18.5], [0.0, 0.4, 18.5], [9.0, 0.4, 18.5]];
    let base = u32::try_from(m.vertices.len()).unwrap();
    m.vertices.extend(flat);
    m.triangles.push([base, base + 1, base + 2]);
    let p = made(&m, &Options::new(1.0));
    assert!(p.unhit > 0);
    let corners: Vec<[Point; 3]> =
        m.triangles.iter().map(|t| t.map(|i| m.vertices[i as usize])).collect();
    let (with_area, _) = corners.split_at(corners.len() - 1);
    let t = p.set.topology();
    let mut nearer = 0;
    for n in t.leaf_numbers() {
        for (axis, c) in p.set.axes().iter().enumerate() {
            for (i, v) in t.leaf_voxels(n).unwrap().enumerate() {
                if c.fraction.leaf(t, n).unwrap()[i] < 0.0 {
                    continue;
                }
                let a = v.map(f64::from);
                let mut b = a;
                b[axis] += 1.0;
                if corners.iter().any(|tri| crossing(a, b, *tri).is_some()) {
                    continue;
                }
                let best =
                    with_area.iter().map(|tri| nearest(a, b, *tri).1).fold(f64::MAX, f64::min);
                nearer += usize::from(nearest(a, b, flat).1 < best);
            }
        }
    }
    assert!(nearer > 0, "the flat triangle is nearest no unhit edge");
}

#[test]
fn of_an_edge_whose_lower_end_is_outside_the_crossing_nearest_its_upper_end_is_kept() {
    // As above, the edge from -5.1 to -4.1 along x: outside at its lower end, it crosses
    // the wall at -5 and -4.4 and both solids at -4.3; the last, nearest its inside end,
    // from the first solid, is kept.
    let m = boxes(&[
        ([-5.0; 3], [5.0; 3], false),
        ([-4.4; 3], [4.4; 3], true),
        ([-4.3; 3], [4.3; 3], false),
        ([-4.3; 3], [4.3; 3], false),
    ]);
    let o = Options::new(1.0).with_origin([-0.1, 0.25, 0.25]);
    let p = made(&m, &o);
    let t = p.set.topology();
    let v = [-5, 0, 0];
    assert_eq!(p.set.signs().value_at(t, v).unwrap(), 0);
    assert_eq!(p.set.signs().value_at(t, [-4, 0, 0]).unwrap(), 1);
    let (n, i) = place(t, v);
    let x = &p.set.axes()[0];
    assert!(
        (f64::from(x.fraction.leaf(t, n).unwrap()[i]) - 0.8).abs() <= FRACTION_TOLERANCE * 10.0
    );
    assert_eq!(x.group.leaf(t, n).unwrap()[i], 3);
    assert_eq!(
        x.normal.leaf(t, n).unwrap()[i].map(f32::to_bits),
        [-1.0_f32, 0.0, 0.0].map(f32::to_bits)
    );
}

#[test]
fn a_normal_perpendicular_to_its_edge_keeps_its_triangle_s_side() {
    // A flat rectangle facing +z, open: points on it are inside, points beside it in its
    // plane outside. The x edges into and out of it lie in its plane and hit nothing; their
    // crossings are at the ends on it, their normals the rectangle's, along neither edge.
    let vertices = vec![[0.5, 0.5, 0.0], [2.5, 0.5, 0.0], [2.5, 3.5, 0.0], [0.5, 3.5, 0.0]];
    let m = Mesh::new(vertices, vec![[0, 1, 2], [0, 2, 3]]).with_unit(Some("millimeter".into()));
    let o = Options::new(1.0).with_accuracy(Accuracy::Exact);
    let p = made(&m, &o);
    let t = p.set.topology();
    let x = &p.set.axes()[0];
    // Into it, from outside, and out of it, from inside.
    for (v, inside) in [([0, 1, 0], 0), ([2, 1, 0], 1)] {
        assert_eq!(p.set.signs().value_at(t, v).unwrap(), inside, "{v:?}");
        let (n, i) = place(t, v);
        assert!(x.fraction.leaf(t, n).unwrap()[i] >= 0.0, "{v:?} has no crossing");
        assert_eq!(x.normal.leaf(t, n).unwrap()[i][2].to_bits(), 1.0_f32.to_bits(), "{v:?}");
    }
}

/// The set's own metadata (U1 of P5's review): the conversion's lattice and unit, named
/// `hermite`, without the SDF convention's key, which `entries` refuses.
#[test]
fn the_set_is_written_with_its_own_metadata() {
    let m = common::torus(20.0, 7.0, 32);
    let o = Options::new(1.0).with_origin([0.25, -0.5, 3.0]);
    let c = convert(&m, &o).unwrap();
    let p = hermite(&m, &c).unwrap();
    assert_eq!(p.meta.name, "hermite");
    assert_eq!((p.meta.voxel_size, p.meta.transform), (c.meta.voxel_size, c.meta.transform));
    assert_eq!(p.meta.unit, c.meta.unit);
    assert!(!p.meta.extra.contains_key(SDF_KEY));
    assert!(p.set.entries("torus", &p.meta).is_ok());
    assert!(matches!(p.set.entries("torus", &c.meta), Err(HermiteError::ClaimsSdf)));
}

/// A conversion a user changed, with a leaf at the end of the `i32` range (C3 of P5's
/// review): the edge beyond its last points reaches the background, sign 0, as brodo takes
/// it, rather than overflowing.
#[test]
fn a_leaf_at_the_end_of_the_range_has_the_background_beyond() {
    let m = common::torus(20.0, 7.0, 32);
    let o = Options::new(1.0);
    let mut c = convert(&m, &o).unwrap();
    let mut e = c.topology.edit();
    e.activate([i32::MAX, 0, 0]).unwrap();
    let (t, r) = e.commit().unwrap();
    c.field = c.field.remap(&r, &brodo::field::uniform).unwrap();
    c.topology = t;
    let p = hermite(&m, &c).unwrap();
    assert_eq!(p.set.signs().value_at(p.set.topology(), [i32::MAX, 0, 0]), Ok(0));
}

/// A box across the end of the `i32` range (the final mutation run of P5): the
/// conversion moved so that voxel x = `i32::MAX` lies at world x = 0, inside a box whose
/// face is at 0.5. The edge from it to the point beyond the range, the background, crosses
/// that face half way, and is hit.
#[test]
fn an_edge_to_beyond_the_range_crosses_where_the_mesh_does() {
    let m = boxes(&[([-1.5, 2.5, 2.5], [0.5, 5.5, 5.5], false)]);
    let mut c = convert(&m, &Options::new(1.0)).unwrap();
    let empty = brodo::topology::Topology::new();
    let mut e = empty.edit();
    e.activate([i32::MAX - 7, 0, 0]).unwrap();
    c.topology = e.commit().unwrap().0;
    // Hermite data reads the conversion's topology, not its field's values.
    c.field = brodo::field::Field::new(&c.topology, 3.0);
    c.meta.transform[0][3] = -f64::from(i32::MAX);
    let p = hermite(&m, &c).unwrap();
    assert_eq!(p.unhit, 0);
    let (t, x) = (p.set.topology(), &p.set.axes()[0]);
    let half = x.fraction.value_at(t, [i32::MAX, 3, 3]).unwrap();
    assert!((half - 0.5).abs() < 1e-6, "{half}");
}

/// Hermite data follows its conversion's accuracy (Q5 of P5's review): made from a
/// conversion at a coarse β, every leaf point's sign is the winding number's at that β, as
/// the conversion records it, not at β = 2.
#[test]
fn signs_follow_the_conversions_accuracy() {
    let m = common::torus(9.0, 3.5, 24);
    let coarse = Accuracy::Approximate { beta: BETA };
    let o = Options::new(1.0).with_accuracy(coarse);
    let c = convert(&m, &o).unwrap();
    assert_eq!(c.accuracy, coarse);
    let p = hermite(&m, &c).unwrap();
    let w = colino::winding::Winding::new(&m).unwrap();
    let t = p.set.topology();
    let mut apart = 0;
    for n in t.leaf_numbers() {
        let signs = p.set.signs().leaf(t, n).unwrap();
        for (v, s) in t.leaf_voxels(n).unwrap().zip(signs) {
            let at = v.map(f64::from);
            let inside = w.number(at, coarse) >= 0.5;
            assert_eq!(*s, u8::from(inside), "{v:?}");
            apart +=
                usize::from(inside != (w.number(at, Accuracy::Approximate { beta: 2.0 }) >= 0.5));
        }
    }
    // The coarse β decides some point otherwise than β = 2, so the test tells them apart.
    assert!(apart > 0);
}

/// A β coarse enough to decide some points otherwise than β = 2.
const BETA: f64 = 0.3;
