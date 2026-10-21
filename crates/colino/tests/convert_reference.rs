//! Reference tier (TESTING.md §1) for conversion (ADR-0027): every voxel of a dense box
//! around a closed mesh holds what a search of every triangle and the exact winding number
//! say, band, tiles and background alike; on open soups, the leaves are the cells a voxel
//! of which lies in the band, holding the same values; a sphere and a torus keep their
//! Betti numbers; the bytes do not depend on the thread count.

use brodo::analysis::{Connectivity, Label, label};
use brodo::euler::euler;
use brodo::topology::Location;
use brodo::tree::Coord;
use colino::convert::{Converted, Options, convert};
use colino::distance::{Point, on_segment, squared};
use colino::error::ConvertError;
use colino::mesh::Mesh;
use colino::winding::{Accuracy, Winding};
use proptest::prelude::*;

mod common;
use common::{sphere, torus};

fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: Point, b: Point) -> Point {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot(a: Point, b: Point) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The distance to the mesh by every triangle, projected to its plane or its nearest edge.
/// Triangles whose box lies farther than the best so far are skipped.
fn distance(mesh: &Mesh, p: Point) -> f64 {
    let mut best = f64::INFINITY;
    for t in &mesh.triangles {
        let [a, b, c] = t.map(|i| mesh.vertices[i as usize]);
        let gap = |k: usize| {
            let (lo, hi) = (a[k].min(b[k]).min(c[k]), a[k].max(b[k]).max(c[k]));
            (lo - p[k]).max(p[k] - hi).max(0.0)
        };
        if gap(0) * gap(0) + gap(1) * gap(1) + gap(2) * gap(2) >= best {
            continue;
        }
        let edges = [(a, b), (b, c), (c, a)];
        let mut d = edges
            .iter()
            .map(|(u, v)| squared(p, on_segment(p, *u, *v)))
            .fold(f64::INFINITY, f64::min);
        let n = cross(sub(b, a), sub(c, a));
        let nn = dot(n, n);
        if nn > 0.0 {
            let s = dot(sub(p, a), n) / nn;
            let q = [p[0] - s * n[0], p[1] - s * n[1], p[2] - s * n[2]];
            if edges.iter().all(|(u, v)| dot(cross(sub(*v, *u), sub(q, *u)), n) >= 0.0) {
                d = d.min(squared(p, q));
            }
        }
        best = best.min(d);
    }
    best.sqrt()
}

/// What a voxel holds by the reference, before the band and tiles are decided.
#[expect(clippy::cast_possible_truncation, reason = "values are stored as f32, as converted")]
fn expected(mesh: &Mesh, w: &Winding, o: &Options, v: Coord) -> f32 {
    let p = [0, 1, 2].map(|k| o.origin[k] + o.voxel_size * (f64::from(v[k]) + 0.5));
    let width = f64::from(o.band) * o.voxel_size;
    let d = distance(mesh, p);
    let signed = if w.number(p, Accuracy::Exact) >= 0.5 { -d } else { d };
    signed.clamp(-width, width) as f32
}

#[expect(clippy::cast_possible_truncation, reason = "the test meshes lie near the origin")]
/// The cells of a box around the mesh, grown by the band, in index space.
fn cells(mesh: &Mesh, o: &Options) -> Vec<Coord> {
    let band = f64::from(o.band) + 1.0;
    let lo = [0, 1, 2].map(|k| {
        mesh.vertices
            .iter()
            .map(|v| (v[k] - o.origin[k]) / o.voxel_size)
            .fold(f64::INFINITY, f64::min)
            - band
    });
    let hi = [0, 1, 2].map(|k| {
        mesh.vertices
            .iter()
            .map(|v| (v[k] - o.origin[k]) / o.voxel_size)
            .fold(f64::NEG_INFINITY, f64::max)
            + band
    });
    let range =
        |k: usize| ((lo[k].floor() as i32).div_euclid(8))..=((hi[k].ceil() as i32).div_euclid(8));
    let mut out = Vec::new();
    for z in range(2) {
        for y in range(1) {
            for x in range(0) {
                out.push([x * 8, y * 8, z * 8]);
            }
        }
    }
    out
}

fn voxels(cell: Coord) -> impl Iterator<Item = Coord> {
    (0..512).map(move |i| [cell[0] + i % 8, cell[1] + i / 8 % 8, cell[2] + i / 64])
}

#[expect(clippy::cast_possible_truncation, reason = "the band's width as f32, as converted")]
/// Checks a conversion against the reference: leaves where a voxel is in the band, with
/// the reference's values within `tolerance`; where `closed`, every other voxel too, ±N·h
/// by the reference's sign.
fn check(
    mesh: &Mesh,
    o: &Options,
    c: &Converted,
    closed: bool,
    tolerance: f32,
) -> Result<(), String> {
    let w = Winding::new(mesh).unwrap();
    let edge = (f64::from(o.band) * o.voxel_size) as f32;
    for cell in cells(mesh, o) {
        let want: Vec<f32> = voxels(cell).map(|v| expected(mesh, &w, o, v)).collect();
        let leaf = want.iter().any(|x| x.abs() < edge);
        let is_leaf = matches!(c.topology.locate(cell), Location::Leaf(_));
        if leaf != is_leaf {
            return Err(format!("cell {cell:?}: leaf {is_leaf}, expected {leaf}"));
        }
        if !leaf && !closed {
            continue;
        }
        for (v, x) in voxels(cell).zip(&want) {
            let got = c.field.value_at(&c.topology, v).map_err(|e| e.to_string())?;
            let close =
                if leaf { (got - x).abs() <= tolerance } else { got.to_bits() == x.to_bits() };
            if !close {
                return Err(format!("voxel {v:?}: {got}, expected {x}"));
            }
        }
    }
    Ok(())
}

fn exact(h: f64) -> Options {
    Options { accuracy: Accuracy::Exact, ..Options::new(h) }
}

#[test]
fn a_sphere_converts_as_the_reference_says_everywhere() {
    // Large enough for inside tiles: a lower node's cell lies wholly 3 voxels inside.
    let m = sphere(2, 18.0, [0.3, -0.2, 0.1]);
    let o = exact(1.0);
    let c = convert(&m, &o).unwrap();
    assert!(c.topology.tile_count() > 0);
    check(&m, &o, &c, true, 1e-5).unwrap();
    assert_eq!(c.meta.unit, "millimeter");
    assert_eq!(c.meta.voxel_size.map(f64::to_bits), [1.0_f64; 3].map(f64::to_bits));
}

#[test]
fn a_larger_sphere_has_inside_tiles_and_the_approximation_agrees() {
    let m = sphere(4, 20.0, [0.0; 3]);
    let c = convert(&m, &Options::new(1.0)).unwrap();
    assert!(c.topology.tile_count() > 0);
    assert_eq!(c.field.value_at(&c.topology, [0, 0, 0]).unwrap().to_bits(), (-3.0_f32).to_bits());
    assert_eq!(c.field.value_at(&c.topology, [60, 0, 0]).unwrap().to_bits(), 3.0_f32.to_bits());
    let e = convert(&m, &exact(1.0)).unwrap();
    for n in e.topology.leaf_numbers() {
        let o = e.topology.leaf_origin(n).unwrap();
        for (v, x) in e.topology.leaf_voxels(n).unwrap().zip(e.field.leaf(&e.topology, n).unwrap())
        {
            assert_eq!(
                c.field.value_at(&c.topology, v).unwrap().to_bits(),
                x.to_bits(),
                "{v:?} in {o:?}"
            );
        }
    }
}

/// The solid's Betti numbers: components 26-connected, cavities as the complement's
/// components, 6-connected, that do not reach the outside, and tunnels from χ.
fn betti(c: &Converted) -> [i64; 3] {
    let solid = |v: f32| v < 0.0;
    let b0 =
        i64::from(label(&c.topology, &c.field, solid, Connectivity::TwentySix).unwrap().count());
    let out = label(&c.topology, &c.field, |v| !solid(v), Connectivity::Six).unwrap();
    let b2 = i64::try_from(
        (0..out.count()).filter(|k| !out.reaches_outside(Label::Component(*k))).count(),
    )
    .unwrap();
    let b1 = b0 + b2 - euler(&c.topology, &c.field, solid, Connectivity::TwentySix).unwrap();
    [b0, b1, b2]
}

#[test]
fn a_sphere_and_a_torus_keep_their_betti_numbers() {
    for (name, mesh, want) in [
        ("sphere", sphere(4, 20.0, [0.0; 3]), [1, 0, 0]),
        ("torus", torus(20.0, 7.0, 48), [1, 1, 0]),
    ] {
        let c = convert(&mesh, &Options::new(1.0)).unwrap();
        assert_eq!(betti(&c), want, "{name}");
    }
}

/// A sphere of radius 20 with a hole in its cap, the triangles whose middle lies above
/// z = 18 taken out (ROADMAP.md, P2's acceptance). The winding number spans the hole, so
/// the sign does not leak through it: the solid is still one ball, and every voxel more
/// than three voxels from the hole has the sign the closed sphere gives it.
#[test]
fn a_holed_mesh_leaks_no_sign() {
    let closed = sphere(4, 20.0, [0.0; 3]);
    let mut holed = closed.clone();
    let middle_z =
        |t: &[u32; 3]| t.iter().map(|i| closed.vertices[*i as usize][2]).sum::<f64>() / 3.0;
    holed.triangles.retain(|t| middle_z(t) <= 18.0);
    assert!(holed.triangles.len() < closed.triangles.len());
    let (a, b) = [&closed, &holed].map(|m| convert(m, &Options::new(1.0)).unwrap()).into();
    assert_eq!(betti(&b), [1, 0, 0]);
    // The hole's rim lies at z ≈ 18, within √(20² − 18²) ≈ 8.7 of the axis.
    let near_hole = |v: Coord| {
        let p = v.map(|c| f64::from(c) + 0.5);
        p[2] > 18.0 - 3.0 - 1.0 && p[0].hypot(p[1]) < 8.8 + 3.0
    };
    let (mut compared, mut inside) = (0, 0);
    for z in -26..26 {
        for y in -26..26 {
            for x in -26..26 {
                let v = [x, y, z];
                if near_hole(v) {
                    continue;
                }
                let (p, q) = (
                    a.field.value_at(&a.topology, v).unwrap(),
                    b.field.value_at(&b.topology, v).unwrap(),
                );
                assert_eq!(p < 0.0, q < 0.0, "at {v:?}: closed {p}, holed {q}");
                compared += 1;
                inside += usize::from(p < 0.0);
            }
        }
    }
    assert!(compared > 100_000 && inside > 25_000, "{compared} voxels, {inside} inside");
}

#[test]
fn the_bytes_do_not_depend_on_the_thread_count() {
    let m = torus(20.0, 7.0, 32);
    let run = |threads| {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
        pool.install(|| convert(&m, &Options::new(1.0)).unwrap())
    };
    let (one, many) = (run(1), run(8));
    let bytes = |c: &Converted| {
        let mut out: Vec<u32> = Vec::new();
        for n in c.topology.leaf_numbers() {
            out.extend(c.topology.leaf_origin(n).unwrap().map(|x| x.cast_unsigned()));
            out.extend(c.field.leaf(&c.topology, n).unwrap().iter().map(|x| x.to_bits()));
        }
        for n in c.topology.tile_numbers() {
            out.extend(c.topology.tile(n).unwrap().1.map(|x| x.cast_unsigned()));
        }
        out
    };
    assert_eq!(bytes(&one), bytes(&many));
}

#[test]
fn bad_options_and_a_missing_unit_are_refused() {
    let m = sphere(1, 5.0, [0.0; 3]);
    let refused = |o: Options| convert(&m, &o).err();
    assert!(matches!(refused(Options::new(0.0)), Some(ConvertError::BadOption(_))));
    assert!(matches!(refused(Options::new(f64::NAN)), Some(ConvertError::BadOption(_))));
    assert!(matches!(
        refused(Options { band: 0, ..Options::new(1.0) }),
        Some(ConvertError::BadOption(_))
    ));
    assert!(matches!(
        refused(Options { origin: [f64::INFINITY, 0.0, 0.0], ..Options::new(1.0) }),
        Some(ConvertError::BadOption(_))
    ));
    assert!(matches!(
        refused(Options { accuracy: Accuracy::Approximate { beta: 0.0 }, ..Options::new(1.0) }),
        Some(ConvertError::BadOption(_))
    ));
    let bare = Mesh { unit: None, ..m.clone() };
    assert_eq!(convert(&bare, &Options::new(1.0)).err(), Some(ConvertError::NoUnit));
    assert!(convert(&bare, &Options { unit: Some("inch".into()), ..Options::new(1.0) }).is_ok());
    let far = sphere(1, 5.0, [1e12, 0.0, 0.0]);
    assert_eq!(convert(&far, &Options::new(1.0)).err(), Some(ConvertError::OutOfRange));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]

    #[test]
    fn soups_band_as_the_reference_says(
        soup in prop::collection::vec([[-4.0..4.0_f64, -4.0..4.0, -4.0..4.0], [-4.0..4.0_f64, -4.0..4.0, -4.0..4.0], [-4.0..4.0_f64, -4.0..4.0, -4.0..4.0]], 1..8),
        h in prop_oneof![Just(0.5), Just(1.0), Just(0.75)],
        origin in [-1.0..1.0_f64, -1.0..1.0, -1.0..1.0],
        band in 1..4_u32,
    ) {
        let vertices: Vec<Point> = soup.iter().flatten().copied().collect();
        let triangles = (0..u32::try_from(soup.len()).unwrap()).map(|i| [3 * i, 3 * i + 1, 3 * i + 2]).collect();
        let m = Mesh { vertices, triangles, unit: Some("m".into()) };
        let o = Options { origin, band, ..exact(h) };
        let c = convert(&m, &o).unwrap();
        prop_assert_eq!(check(&m, &o, &c, false, 1e-5), Ok(()));
    }
}

/// A mesh with no triangle converts to the background alone.
#[test]
fn a_mesh_with_no_triangle_converts_to_nothing() {
    let m = Mesh { vertices: vec![[1.0, 2.0, 3.0]], triangles: vec![], unit: Some("mm".into()) };
    let counts =
        convert(&m, &Options::new(1.0)).map(|c| (c.topology.leaf_count(), c.topology.tile_count()));
    assert_eq!(counts, Ok((0, 0)));
}

/// A small triangle at `p`, in the mesh's unit.
fn speck(p: Point) -> Mesh {
    Mesh {
        vertices: vec![p, [p[0] + 0.01, p[1], p[2]], [p[0], p[1] + 0.01, p[2]]],
        triangles: vec![[0, 1, 2]],
        unit: Some("millimeter".into()),
    }
}

/// Index space reaches 2³¹ − 1 less two root regions, 8192 voxels, a margin the walk
/// down from a root region needs: a mesh whose box, grown by the band, ends exactly there
/// converts, and one a voxel further is refused.
#[test]
fn index_space_ends_two_root_regions_short_of_the_i32_range() {
    let limit = f64::from(i32::MAX) - 8192.0;
    let o = Options::new(1.0);
    let at_edge = convert(&speck([limit - 3.01, 0.0, 0.0]), &o);
    assert_eq!(at_edge.map(|c| c.topology.leaf_count() > 0), Ok(true));
    let beyond = convert(&speck([limit - 2.0, 0.0, 0.0]), &o).map(|_| ());
    assert_eq!(beyond, Err(ConvertError::OutOfRange));
    let far = convert(&speck([-limit, 0.0, 0.0]), &o).map(|_| ());
    assert_eq!(far, Err(ConvertError::OutOfRange));
}

/// A cell's corner voxel within the band of a triangle beyond that corner is found,
/// though the cell's centre lies further than the band and the voxels' reach from it:
/// the corner voxel is 2.9 voxels from the triangle, the cell's centre 8.96, at voxels of
/// one unit and of two.
#[test]
fn a_cell_reached_only_at_its_corner_is_a_leaf() {
    for h in [1.0, 2.0] {
        let m = speck([(7.5 + 2.9 / 3.0_f64.sqrt()) * h; 3]);
        let o = exact(h);
        let c = convert(&m, &o).unwrap();
        assert!(matches!(c.topology.locate([0, 0, 0]), Location::Leaf(_)), "h = {h}");
        assert_eq!(check(&m, &o, &c, false, 1e-5), Ok(()), "h = {h}");
    }
}

/// Spheres of radius 28, whose inside tiles reach two cells from the centre: one about the
/// origin, one across the root regions' boundary at x = 4096. Every voxel is as the
/// reference says, tiles included.
#[test]
fn larger_spheres_convert_as_the_reference_says_everywhere() {
    for x in [0, 4096, -100] {
        let m = sphere(2, 28.0, [f64::from(x) + 0.3, -0.2, 0.1]);
        let o = exact(1.0);
        let c = convert(&m, &o).unwrap();
        for v in [[x - 12, 0, 0], [x + 12, 0, 0], [x, -12, 0], [x, 0, 12]] {
            assert!(matches!(c.topology.locate(v), Location::Tile(..)), "{v:?}");
        }
        assert_eq!(check(&m, &o, &c, true, 1e-5), Ok(()), "{x}");
    }
}

/// A cube from -5 to 37: its box, grown by the band, ends at 40, in the fifth cell, and
/// an inside tile lies in the cell below the one before it, which the walk must reach.
#[test]
fn a_cubes_inside_tiles_reach_near_its_box() {
    let (lo, hi) = (-5.0, 37.0);
    let corner = |i: u32| [0, 1, 2].map(|k| if i >> k & 1 == 1 { hi } else { lo });
    let vertices: Vec<Point> = (0..8).map(corner).collect();
    // Each face's corners, counter-clockwise seen from outside.
    let faces =
        [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
    let triangles = faces.iter().flat_map(|[a, b, c, d]| [[*a, *b, *c], [*a, *c, *d]]).collect();
    let m = Mesh { vertices, triangles, unit: Some("millimeter".into()) };
    let o = exact(1.0);
    let c = convert(&m, &o).unwrap();
    assert!(matches!(c.topology.locate([24, 8, 8]), Location::Tile(..)));
    assert_eq!(check(&m, &o, &c, true, 1e-5), Ok(()));
}
