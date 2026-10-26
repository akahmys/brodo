//! Reference tier (TESTING.md §1) for the parity sign (ADR-0027): turning any of a mesh's
//! triangles leaves its conversion by parity unchanged, to the bit; on closed, well
//! oriented meshes parity and the winding number convert alike; a sphere with a hole and
//! turned faces keeps its inside below the hole; a line through two holes is outvoted;
//! the bytes do not depend on the thread count.

mod common;

use brodo::topology::Topology;
use brodo::tree::Coord;
use colino::convert::{Converted, Options, Sign, convert};
use colino::mesh::Mesh;
use common::{sphere, torus};

/// A conversion's leaves, their values and its tiles, as bits.
fn bytes(c: &Converted) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::new();
    for n in c.topology.leaf_numbers() {
        out.extend(c.topology.leaf_origin(n).unwrap().map(|x| x.cast_unsigned()));
        out.extend(c.field.leaf(&c.topology, n).unwrap().iter().map(|x| x.to_bits()));
    }
    for n in c.topology.tile_numbers() {
        out.extend(c.topology.tile(n).unwrap().1.map(|x| x.cast_unsigned()));
    }
    out
}

/// A fixed pseudo-random choice of about `permille` thousandths of `n` items.
fn chosen(n: usize, permille: u64, seed: u64) -> Vec<bool> {
    let mut g = seed;
    (0..n)
        .map(|_| {
            g = g.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            (g >> 33) % 1000 < permille
        })
        .collect()
}

fn turned(m: &Mesh, which: &[bool]) -> Mesh {
    let mut out = m.clone();
    for (t, turn) in out.triangles.iter_mut().zip(which) {
        if *turn {
            t.swap(1, 2);
        }
    }
    out
}

fn parity(h: f64) -> Options {
    Options::new(h).with_sign(Sign::Parity)
}

fn meshes() -> Vec<Mesh> {
    let unit = |m: Mesh| m.with_unit(Some("millimeter".into()));
    vec![unit(sphere(4, 9.7, [0.3, 0.1, -0.2])), unit(torus(12.0, 4.1, 32))]
}

#[test]
fn turning_triangles_leaves_a_parity_conversion_unchanged() {
    for m in meshes() {
        let o = parity(0.7);
        let as_meant = bytes(&convert(&m, &o).unwrap());
        let n = m.triangles.len();
        for (p, seed) in [(100, 1), (500, 2), (1000, 3)] {
            let c = convert(&turned(&m, &chosen(n, p, seed)), &o).unwrap();
            assert_eq!(bytes(&c), as_meant, "{p} permille of {n}");
        }
    }
}

#[test]
fn parity_and_the_winding_number_convert_a_closed_mesh_alike() {
    let mut tiles = Vec::new();
    for m in meshes() {
        let by_winding = convert(&m, &Options::new(0.4)).unwrap();
        let by_parity = convert(&m, &parity(0.4)).unwrap();
        assert_eq!((by_winding.sign, by_parity.sign), (Sign::Winding, Sign::Parity));
        tiles.push(by_parity.topology.tile_numbers().count());
        assert_eq!(bytes(&by_parity), bytes(&by_winding));
    }
    // The sphere's inside regions are classified by parity too.
    assert!(tiles[0] > 0, "{tiles:?}");
}

/// Whether voxel `v` is inside a conversion: a negative value.
fn inside(t: &Topology, c: &Converted, v: Coord) -> bool {
    c.field.value_at(t, v).unwrap() < 0.0
}

#[test]
fn a_sphere_with_a_hole_and_turned_faces_keeps_its_inside_below_the_hole_by_parity() {
    // A hole at the top, its rim at z = 9.3, and a tenth of the rest turned. Below the rim
    // every sign is the sphere's as meant; above it, within the missing cap, a line across
    // passes through the hole and finds nothing, so the cap may be outside.
    let m = sphere(4, 9.7, [0.3, 0.1, -0.2]).with_unit(Some("millimeter".into()));
    let n = m.triangles.len();
    let rim = 9.3;
    let top = |t: &[u32; 3]| t.iter().all(|i| m.vertices[*i as usize][2] > rim);
    let mut broken = turned(&m, &chosen(n, 100, 4));
    broken.triangles.retain(|t| !top(t));
    assert!(broken.triangles.len() < n - 4);
    let h = 0.7;
    let as_meant = convert(&m, &Options::new(h)).unwrap();
    let c = convert(&broken, &parity(h)).unwrap();
    let (mut below, mut cap) = (0, 0);
    for x in -20..20 {
        for y in -20..20 {
            for z in -20..20 {
                let v = [x, y, z];
                let differ = inside(&c.topology, &c, v) != inside(&as_meant.topology, &as_meant, v);
                if (f64::from(z) + 0.5) * h <= rim {
                    assert!(!differ, "{v:?}");
                    below += 1;
                } else {
                    cap += usize::from(differ);
                }
            }
        }
    }
    assert!(below > 50_000 && cap < 20, "{below} below, {cap} in the cap");
}

#[test]
fn a_line_through_two_holes_is_outvoted_by_the_other_two() {
    // Caps removed at both ends of the x axis, their rims at |x| = 9.3: a line along x
    // through the inside passes through both holes and crosses nothing, an even count that
    // votes outside; the lines along y and z outvote it.
    let m = sphere(4, 9.7, [0.3, 0.1, -0.2]).with_unit(Some("millimeter".into()));
    let rim = 9.3;
    let end = |t: &[u32; 3]| t.iter().all(|i| (m.vertices[*i as usize][0] - 0.3).abs() > rim);
    let mut broken = m.clone();
    broken.triangles.retain(|t| !end(t));
    assert!(broken.triangles.len() < m.triangles.len() - 8);
    let h = 0.7;
    let as_meant = convert(&m, &Options::new(h)).unwrap();
    let c = convert(&broken, &parity(h)).unwrap();
    let mut through = 0;
    for x in -20..20 {
        for y in -3..3 {
            for z in -3..3 {
                let v = [x, y, z];
                if ((f64::from(x) + 0.5) * h - 0.3).abs() > rim {
                    continue;
                }
                let meant = inside(&as_meant.topology, &as_meant, v);
                assert_eq!(inside(&c.topology, &c, v), meant, "{v:?}");
                through += usize::from(meant);
            }
        }
    }
    assert!(through > 500, "{through} points inside on lines through both holes");
}

#[test]
fn the_bytes_by_parity_do_not_depend_on_the_thread_count() {
    let m = torus(20.0, 7.0, 32).with_unit(Some("millimeter".into()));
    let run = |threads| {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
        pool.install(|| convert(&m, &parity(1.0)).unwrap())
    };
    assert_eq!(bytes(&run(1)), bytes(&run(8)));
}
