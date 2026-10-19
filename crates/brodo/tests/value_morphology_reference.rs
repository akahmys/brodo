//! Reference tier (TESTING.md §1): dilating and eroding values (ADR-0025) gives, bit for
//! bit, what a max or min filter over a dense grid does when only leaf voxels are written:
//! each takes the largest or smallest of itself and its 6 or 26 neighbours, a NaN among
//! them winning, k times over; tiles and the background keep their values.

use brodo::analysis::Connectivity;
use brodo::field::Field;
use brodo::morphology::{dilate_values, erode_values};
use brodo::topology::{Location, Topology};
use brodo::tree::{Coord, TileLevel};
use proptest::prelude::*;

/// The dense box: voxels from LO to LO + SIDE - 1, a leaf-wide margin around every leaf.
const LO: i32 = -24;
const SIDE: usize = 48;

fn index(v: Coord) -> Option<usize> {
    let [x, y, z] = v.map(|c| usize::try_from(c - LO).ok().filter(|c| *c < SIDE));
    Some(x? + SIDE * (y? + SIDE * z?))
}

fn voxel(i: usize) -> Coord {
    [i % SIDE, i / SIDE % SIDE, i / (SIDE * SIDE)].map(|c| i32::try_from(c).unwrap() + LO)
}

fn neighbours(conn: Connectivity) -> Vec<[i32; 3]> {
    (-1..=1)
        .flat_map(|z| (-1..=1).flat_map(move |y| (-1..=1).map(move |x| [x, y, z])))
        .filter(|o: &[i32; 3]| {
            let n: i32 = o.iter().map(|c| c.abs()).sum();
            n > 0 && (conn == Connectivity::TwentySix || n == 1)
        })
        .collect()
}

/// The dense filter on `f32`: NaN first, then the larger or the smaller.
fn dense(t: &Topology, f: &Field<f32>, conn: Connectivity, k: u32, larger: bool) -> Vec<f32> {
    let mut values: Vec<f32> = (0..SIDE.pow(3)).map(|i| f.value_at(t, voxel(i)).unwrap()).collect();
    let leaf: Vec<bool> =
        (0..SIDE.pow(3)).map(|i| matches!(t.locate(voxel(i)), Location::Leaf(_))).collect();
    let near = neighbours(conn);
    for _ in 0..k {
        let old = values.clone();
        for i in (0..old.len()).filter(|i| leaf[*i]) {
            let v = voxel(i);
            let mut best = old[i];
            for o in &near {
                let w = index([v[0] + o[0], v[1] + o[1], v[2] + o[2]])
                    .map_or(f.background(), |j| old[j]);
                if best.is_nan() {
                    continue;
                }
                if w.is_nan() || (larger && w > best) || (!larger && w < best) {
                    best = w;
                }
            }
            values[i] = best;
        }
    }
    values
}

fn coord() -> impl Strategy<Value = Coord> {
    [-16..16_i32, -16..16, -16..16]
}

/// A small set of values, so that ties are common.
fn value() -> impl Strategy<Value = f32> {
    (-3..=3_i8).prop_map(f32::from)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn values_filter_as_a_dense_grid_does(
        leaves in prop::collection::vec(coord(), 1..12),
        tiles in prop::collection::vec((coord(), value()), 0..6),
        background in value(),
        voxels in prop::collection::vec(value(), 512),
        // In some cases, one NaN in a leaf: everywhere one would spread to most voxels.
        nan in prop::option::weighted(0.3, any::<prop::sample::Index>()),
        k in 0..4_u32,
        six in any::<bool>(),
        larger in any::<bool>(),
    ) {
        let empty = Topology::new();
        let mut e = empty.edit();
        for v in &leaves {
            e.activate(*v).unwrap();
        }
        for (v, _) in &tiles {
            let _occupied = e.add_tile(TileLevel::Lower, *v);
        }
        let (t, _) = e.commit().unwrap();
        let mut f = Field::new(&t, background);
        for (j, n) in t.leaf_numbers().collect::<Vec<_>>().into_iter().enumerate() {
            for (i, x) in f.leaf_mut(&t, n).unwrap().iter_mut().enumerate() {
                *x = voxels[(i + 37 * j) % 512];
            }
        }
        if let Some(at) = nan {
            let n = t.leaf_numbers().nth(at.index(t.leaf_count())).unwrap();
            f.leaf_mut(&t, n).unwrap()[at.index(512)] = f32::NAN;
        }
        for (j, n) in t.tile_numbers().collect::<Vec<_>>().into_iter().enumerate() {
            f.set_tile(&t, n, tiles[j % tiles.len()].1).unwrap();
        }
        let conn = if six { Connectivity::Six } else { Connectivity::TwentySix };
        let got = if larger { dilate_values(&t, &f, conn, k) } else { erode_values(&t, &f, conn, k) }.unwrap();
        let want = dense(&t, &f, conn, k, larger);
        for (i, w) in want.iter().enumerate() {
            let g = got.value_at(&t, voxel(i)).unwrap();
            prop_assert_eq!(g.to_bits(), w.to_bits(), "at {:?}", voxel(i));
        }
    }
}

/// An integer field: a single high voxel spreads one step at a time, and erosion takes it
/// back, both bounded by the leaf.
#[test]
fn an_integer_peak_spreads_and_shrinks() {
    let empty = Topology::new();
    let mut e = empty.edit();
    e.activate([0, 0, 0]).unwrap();
    let (t, _) = e.commit().unwrap();
    let mut f = Field::new(&t, 0_u8);
    let n = t.leaf_numbers().next().unwrap();
    f.leaf_mut(&t, n).unwrap()[3 + 8 * 3 + 64 * 3] = 9;
    let count = |g: &Field<u8>| g.leaf(&t, n).unwrap().iter().filter(|x| **x == 9).count();
    assert_eq!(count(&dilate_values(&t, &f, Connectivity::Six, 1).unwrap()), 7);
    assert_eq!(count(&dilate_values(&t, &f, Connectivity::TwentySix, 2).unwrap()), 125);
    assert_eq!(count(&erode_values(&t, &f, Connectivity::Six, 1).unwrap()), 0);
    let grown = dilate_values(&t, &f, Connectivity::TwentySix, 1).unwrap();
    assert_eq!(count(&erode_values(&t, &grown, Connectivity::TwentySix, 1).unwrap()), 1);
}
