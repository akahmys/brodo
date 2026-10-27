//! Reference tier (TESTING.md §1): the Euler characteristic of a sparse grid equals
//! n₀ − n₁ + n₂ − n₃ counted cell by cell on a dense grid (ADR-0024), and known shapes give
//! their known values. The dense box covers every leaf and tile with a margin; the
//! background never meets the condition, so nothing beyond the box is counted.

use brodo::analysis::{AnalysisError, Connectivity, Label, label};
use brodo::euler::euler;
use brodo::field::Field;
use brodo::topology::Topology;
use brodo::tree::{Coord, TileLevel};
use proptest::prelude::*;

/// The dense box: voxels from LO to HI - 1 on each axis, one beyond the leaves and tiles
/// `coord` places, whose voxels lie in -16..16.
const LO: i32 = -17;
const HI: i32 = 17;
const SIDE: usize = 34;

/// χ counted cell by cell: every cell of the lattice is a corner `p` and the axes it spans;
/// the voxels around it take `p`'s coordinate on a spanned axis and either side of it on
/// the others. Closed cubes count a cell any voxel around it meets; open cubes the dual
/// cell, of dimension 3 − d, of one every voxel around it meets.
fn dense(meets: impl Fn(Coord) -> bool, conn: Connectivity) -> i64 {
    let index = |v: Coord| {
        let [x, y, z] = v.map(|c| usize::try_from(c - LO).ok().filter(|c| *c < SIDE));
        Some(x? + SIDE * (y? + SIDE * z?))
    };
    let mut met = vec![false; SIDE * SIDE * SIDE];
    for z in LO..HI {
        for y in LO..HI {
            for x in LO..HI {
                met[index([x, y, z]).unwrap()] = meets([x, y, z]);
            }
        }
    }
    let meets = |v: Coord| index(v).is_some_and(|i| met[i]);
    let mut chi = 0;
    for z in LO..=HI {
        for y in LO..=HI {
            for x in LO..=HI {
                for spans in 0..8_u32 {
                    let around: Vec<Coord> = (0..8_u32)
                        .filter(|side| side & spans == spans)
                        .map(|side| {
                            let at = |axis: u32, c: i32| {
                                if spans >> axis & 1 == 1 || side >> axis & 1 == 1 {
                                    c
                                } else {
                                    c - 1
                                }
                            };
                            [at(0, x), at(1, y), at(2, z)]
                        })
                        .collect();
                    let dim = spans.count_ones();
                    let (counted, dim) = match conn {
                        Connectivity::TwentySix => (around.iter().any(|v| meets(*v)), dim),
                        Connectivity::Six => (around.iter().all(|v| meets(*v)), 3 - dim),
                    };
                    if counted {
                        chi += if dim % 2 == 0 { 1 } else { -1 };
                    }
                }
            }
        }
    }
    chi
}

fn coord() -> impl Strategy<Value = Coord> {
    [-12..12_i32, -12..12, -12..12]
}

const IN: f32 = -1.0;
const OUT: f32 = 1.0;

fn inside(v: f32) -> bool {
    v < 0.0
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn chi_counts_as_the_dense_cells_do(
        leaves in prop::collection::vec(coord(), 1..12),
        tiles in prop::collection::vec(coord(), 0..8),
        seed in any::<u64>(),
        dense_fill in 1..5_u64,
        six in any::<bool>(),
    ) {
        let empty = Topology::new();
        let mut e = empty.edit();
        for v in &leaves {
            e.activate(*v).unwrap();
        }
        for v in &tiles {
            let _occupied = e.add_tile(TileLevel::Lower, *v);
        }
        let (t, _) = e.commit().unwrap();
        let mut f = Field::new(&t, OUT);
        let mut g = seed;
        let mut next = || { g = g.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); g >> 33 };
        for n in t.leaf_numbers().collect::<Vec<_>>() {
            for v in f.leaf_mut(&t, n).unwrap() {
                *v = if next() % 5 < dense_fill { IN } else { OUT };
            }
        }
        for n in t.tile_numbers().collect::<Vec<_>>() {
            f.set_tile(&t, n, if next() % 3 == 0 { OUT } else { IN }).unwrap();
        }
        let conn = if six { Connectivity::Six } else { Connectivity::TwentySix };
        let got = euler(&t, &f, inside, conn).unwrap();
        prop_assert_eq!(got, dense(|v| inside(f.value_at(&t, v).unwrap()), conn));
    }
}

#[test]
fn an_unbounded_set_is_refused() {
    let t = Topology::new();
    let f = Field::new(&t, IN);
    assert_eq!(euler(&t, &f, inside, Connectivity::TwentySix), Err(AnalysisError::Unbounded));
}

/// A grid whose leaves hold the voxels of a box of side 48 around the origin that `shape`
/// says are inside.
fn voxels(shape: impl Fn(Coord) -> bool) -> (Topology, Field<f32>) {
    voxels_within(24, shape)
}

/// The same, in a box of side `2 * half`.
fn voxels_within(half: i32, shape: impl Fn(Coord) -> bool) -> (Topology, Field<f32>) {
    let empty = Topology::new();
    let mut e = empty.edit();
    let side = -half..half;
    for z in side.clone().step_by(8) {
        for y in side.clone().step_by(8) {
            for x in side.clone().step_by(8) {
                e.activate([x, y, z]).unwrap();
            }
        }
    }
    let (t, _) = e.commit().unwrap();
    let mut f = Field::new(&t, OUT);
    for n in t.leaf_numbers().collect::<Vec<_>>() {
        let at: Vec<Coord> = t.leaf_voxels(n).unwrap().collect();
        for (v, value) in at.into_iter().zip(f.leaf_mut(&t, n).unwrap()) {
            *value = if shape(v) { IN } else { OUT };
        }
    }
    (t, f)
}

/// A solid torus around the z axis: ring radius 8, tube radius 3.
fn torus(v: Coord) -> bool {
    let [x, y, z] = v.map(|c| f64::from(c) + 0.5);
    ((x * x + y * y).sqrt() - 8.0).powi(2) + z * z < 9.0
}

/// Whether a voxel is inside a shape.
type Shape = Box<dyn Fn(Coord) -> bool>;

fn squared(v: Coord) -> f64 {
    v.iter().map(|c| (f64::from(*c) + 0.5).powi(2)).sum()
}

#[test]
fn known_shapes_give_their_characteristic() {
    // Two tori side by side, overlapping at the origin: a figure of eight, two handles.
    let double = |v: Coord| torus([v[0] + 8, v[1], v[2]]) || torus([v[0] - 8, v[1], v[2]]);
    let shapes: [(&str, Shape, i64); 4] = [
        ("ball", Box::new(|v| squared(v) < 100.0), 1),
        ("shell", Box::new(|v| (36.0..100.0).contains(&squared(v))), 2),
        ("torus", Box::new(torus), 0),
        ("double torus", Box::new(double), -1),
    ];
    for (name, shape, chi) in shapes {
        let (t, f) = voxels(&shape);
        for conn in [Connectivity::Six, Connectivity::TwentySix] {
            assert_eq!(euler(&t, &f, inside, conn).unwrap(), chi, "{name} under {conn:?}");
        }
    }
}

/// The invariants of ARCHITECTURE.md §5.2 for known shapes, from labelling and χ: β₀ the
/// solid's components, β₂ the complement's components that do not reach the outside (each
/// a cavity), β₁ = β₀ + β₂ − χ. The solid is 26-connected and its complement 6-connected,
/// as colino counts them; labelling is slow in a debug build, so each shape is labelled
/// in the smallest box that holds it.
#[test]
fn labelling_and_chi_give_the_betti_numbers() {
    let double = |v: Coord| torus([v[0] + 8, v[1], v[2]]) || torus([v[0] - 8, v[1], v[2]]);
    let shapes: [(&str, i32, Shape, [i64; 3]); 4] = [
        ("ball", 16, Box::new(|v| squared(v) < 100.0), [1, 0, 0]),
        ("shell", 16, Box::new(|v| (36.0..100.0).contains(&squared(v))), [1, 0, 1]),
        ("torus", 16, Box::new(torus), [1, 1, 0]),
        ("double torus", 24, Box::new(double), [1, 2, 0]),
    ];
    for (name, half, shape, betti) in shapes {
        let (t, f) = voxels_within(half, &shape);
        let (solid, complement) = (Connectivity::TwentySix, Connectivity::Six);
        let b0 = i64::from(label(&t, &f, inside, solid).unwrap().count());
        let outside = label(&t, &f, |v| !inside(v), complement).unwrap();
        let cavities =
            (0..outside.count()).filter(|c| !outside.reaches_outside(Label::Component(*c))).count();
        let b2 = i64::try_from(cavities).unwrap();
        let b1 = b0 + b2 - euler(&t, &f, inside, solid).unwrap();
        assert_eq!([b0, b1, b2], betti, "{name}");
    }
}

/// Tiles too large for the dense box: alone, χ is 1; meeting another region at a corner
/// only, they are one under 26-connectivity and two under 6.
#[test]
fn large_tiles_count_by_their_edges() {
    let at_corner = |level: TileLevel, lone: Option<Coord>, leaf: Option<Coord>| {
        let empty = Topology::new();
        let mut e = empty.edit();
        e.add_tile(level, [0, 0, 0]).unwrap();
        if let Some(v) = lone {
            e.add_tile(TileLevel::Lower, v).unwrap();
        }
        if let Some(v) = leaf {
            e.activate(v).unwrap();
        }
        let (t, _) = e.commit().unwrap();
        let mut f = Field::new(&t, OUT);
        for n in t.tile_numbers().collect::<Vec<_>>() {
            f.set_tile(&t, n, IN).unwrap();
        }
        for n in t.leaf_numbers().collect::<Vec<_>>() {
            // Only the leaf's voxel at its lower corner, touching the tile's corner.
            f.leaf_mut(&t, n).unwrap()[0] = IN;
        }
        [Connectivity::TwentySix, Connectivity::Six].map(|c| euler(&t, &f, inside, c).unwrap())
    };
    for level in [TileLevel::Upper, TileLevel::Root] {
        let e = level.edge();
        assert_eq!(at_corner(level, None, None), [1, 1], "{level:?} alone");
        assert_eq!(at_corner(level, Some([e, e, e]), None), [1, 2], "{level:?} and a tile");
        assert_eq!(at_corner(level, None, Some([e, e, e])), [1, 2], "{level:?} and a leaf");
    }
}

/// A small tile against a large one's face (M1 of P5's review): the small tile's corners
/// lie inside the large tile's face, on no edge of it, so the small tile sums them though
/// the large one holds the first of their voxels. The two make one box: χ = 1.
#[test]
fn a_small_tile_on_a_large_ones_face_sums_its_own_corners() {
    let empty = Topology::new();
    let mut e = empty.edit();
    e.add_tile(TileLevel::Upper, [0, 0, 0]).unwrap();
    e.add_tile(TileLevel::Lower, [128, 8, 8]).unwrap();
    let t = e.commit().unwrap().0;
    let mut f = Field::new(&t, 0_u8);
    for n in t.tile_numbers() {
        f.set_tile(&t, n, 1).unwrap();
    }
    for conn in [Connectivity::Six, Connectivity::TwentySix] {
        assert_eq!(euler(&t, &f, |v| v == 1, conn), Ok(1), "{conn:?}");
    }
}

/// A small tile touching a large one's far edge mid-way, across it (M1 of P5's review):
/// the small tile's corners there lie on the large tile's edge, and the large tile, holding
/// the first of their voxels, sums them; along the rest of the edge the voxels do not
/// change, and its vertices add nothing. One part under 26-connectivity, two under 6.
#[test]
fn a_small_tile_across_a_large_ones_edge_is_summed_by_the_large_one() {
    let empty = Topology::new();
    let mut e = empty.edit();
    e.add_tile(TileLevel::Upper, [-128, -128, -128]).unwrap();
    e.add_tile(TileLevel::Lower, [-8, 0, 0]).unwrap();
    let t = e.commit().unwrap().0;
    let mut f = Field::new(&t, 0_u8);
    for n in t.tile_numbers() {
        f.set_tile(&t, n, 1).unwrap();
    }
    // Within the reference's box, the large tile's corner and the small tile.
    let dense_box = |v: Coord| {
        let large = v.iter().all(|c| *c < 0);
        let small = (-8..0).contains(&v[0]) && v[1..].iter().all(|c| (0..8).contains(c));
        large || small
    };
    for conn in [Connectivity::Six, Connectivity::TwentySix] {
        let expected = dense(dense_box, conn);
        assert_eq!(euler(&t, &f, |v| v == 1, conn), Ok(expected), "{conn:?}");
    }
}
