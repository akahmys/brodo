//! Reference tier (TESTING.md §1): the union of two topologies (`Topology::union`, ADR-0014;
//! issue 8). A model written here holds each topology as plain sets of leaf origins and
//! tiles, read from their listings: the union's leaves are both inputs' leaves, each of its
//! tiles lies under a tile of an input covering it whole and holds no leaf nor smaller tile
//! of either, and together they cover as many voxels as the two inputs cover, so exactly
//! the voxels either covers. Either input's field is moved onto it by `Remap::between`
//! without a merge, its values kept.

use std::collections::BTreeSet;

use brodo::field::{Field, uniform};
use brodo::topology::{Remap, TileSource, Topology};
use brodo::tree::{Coord, TileLevel};
use proptest::prelude::*;

const LEVELS: [TileLevel; 3] = [TileLevel::Root, TileLevel::Upper, TileLevel::Lower];

fn align(v: Coord, log2: u32) -> Coord {
    v.map(|c| c.div_euclid(1 << log2) * (1 << log2))
}

/// A topology of these leaves and tiles, a tile where something already is skipped.
fn build(leaves: &[Coord], tiles: &[(TileLevel, Coord)]) -> Topology {
    let empty = Topology::new();
    let mut e = empty.edit();
    for v in leaves {
        e.activate(*v).unwrap();
    }
    for (l, v) in tiles {
        let _occupied = e.add_tile(*l, *v);
    }
    e.commit().unwrap().0
}

/// A topology's leaves and tiles, each a box: its origin and edge.
struct Model {
    leaves: BTreeSet<Coord>,
    tiles: BTreeSet<(TileLevel, Coord)>,
}

impl Model {
    fn of(t: &Topology) -> Self {
        Self {
            leaves: t.leaf_numbers().map(|n| t.leaf_origin(n).unwrap()).collect(),
            tiles: t.tile_numbers().map(|n| t.tile(n).unwrap()).collect(),
        }
    }

    fn boxes(&self) -> Vec<(Coord, i32)> {
        let leaves = self.leaves.iter().map(|o| (*o, 8));
        leaves.chain(self.tiles.iter().map(|(l, o)| (*o, l.edge()))).collect()
    }

    /// Whether a tile at `level`, or a larger one, covers the region at `level` and `o`.
    fn covers(&self, level: TileLevel, o: Coord) -> bool {
        LEVELS
            .into_iter()
            .filter(|l| l.edge() >= level.edge())
            .any(|l| self.tiles.contains(&(l, align(o, l.edge_log2()))))
    }

    /// Whether a leaf, or a tile smaller than `level`, lies in the region at `level` and
    /// `o`.
    fn finer_in(&self, level: TileLevel, o: Coord) -> bool {
        let within = |c: &Coord| (0..3).all(|k| c[k] >= o[k] && c[k] < o[k] + level.edge());
        self.leaves.iter().any(within)
            || self.tiles.iter().any(|(l, c)| l.edge() < level.edge() && within(c))
    }
}

/// The voxels two boxes share.
fn shared((a, ea): (Coord, i32), (b, eb): (Coord, i32)) -> u128 {
    (0..3)
        .map(|k| {
            let lo = i64::from(a[k].max(b[k]));
            let hi = i64::from((a[k] + ea).min(b[k] + eb));
            u128::try_from((hi - lo).max(0)).unwrap()
        })
        .product()
}

fn volume(boxes: &[(Coord, i32)]) -> u128 {
    boxes.iter().map(|(_, e)| u128::try_from(*e).unwrap().pow(3)).sum()
}

/// A field whose every leaf voxel and tile holds its own value.
fn numbered(t: &Topology, background: u32) -> Field<u32> {
    let mut f = Field::new(t, background);
    for n in t.leaf_numbers() {
        for (i, v) in f.leaf_mut(t, n).unwrap().iter_mut().enumerate() {
            *v = 1000 * (n.get() + 1) + u32::try_from(i).unwrap();
        }
    }
    for n in t.tile_numbers() {
        f.set_tile(t, n, 1_000_000 + n.get()).unwrap();
    }
    f
}

/// Fails unless `u` is the union of `a` and `b` as the model states, and each input's field
/// moves onto it without a merge, keeping its values.
fn is_the_union(a: &Topology, b: &Topology, u: &Topology) -> Result<(), TestCaseError> {
    let (ma, mb, mu) = (Model::of(a), Model::of(b), Model::of(u));
    let leaves: BTreeSet<Coord> = ma.leaves.union(&mb.leaves).copied().collect();
    prop_assert_eq!(&mu.leaves, &leaves);
    for (l, o) in &mu.tiles {
        prop_assert!(ma.covers(*l, *o) || mb.covers(*l, *o), "tile {:?} covers nothing", (l, o));
        prop_assert!(!ma.finer_in(*l, *o) && !mb.finer_in(*l, *o), "tile {:?} hides", (l, o));
    }
    let (ba, bb) = (ma.boxes(), mb.boxes());
    let both: u128 = ba.iter().flat_map(|x| bb.iter().map(move |y| shared(*x, *y))).sum();
    prop_assert_eq!(volume(&mu.boxes()), volume(&ba) + volume(&bb) - both);
    for (t, background) in [(a, 3), (b, 5)] {
        let r = Remap::between(t, u).unwrap();
        prop_assert!(!r.tile_sources().iter().any(|s| matches!(s, TileSource::Merged(_))));
        let f = numbered(t, background);
        let g = f.remap(&r, &uniform).unwrap();
        let points = mu.boxes().into_iter().flat_map(|(o, e)| [o, o.map(|c| c + e - 1)]);
        for v in points {
            prop_assert_eq!(g.value_at(u, v).unwrap(), f.value_at(t, v).unwrap(), "at {:?}", v);
        }
    }
    Ok(())
}

/// Leaves and tiles near the origin, root tiles about it too, so that a root tile of one
/// input holds the other's leaves and smaller tiles.
fn shape() -> impl Strategy<Value = (Vec<Coord>, Vec<(TileLevel, Coord)>)> {
    let near = || (-2..3_i32, -2..3_i32, -2..3_i32);
    let leaves = prop::collection::vec(near().prop_map(|(x, y, z)| [8 * x, 8 * y, 8 * z]), 0..8);
    let lower = near().prop_map(|(x, y, z)| (TileLevel::Lower, [8 * x, 8 * y, 8 * z]));
    let upper = (-1..1_i32, -1..1_i32, -1..1_i32)
        .prop_map(|(x, y, z)| (TileLevel::Upper, [128 * x, 128 * y, 128 * z]));
    let root = (-1..2_i32).prop_map(|x| (TileLevel::Root, [4096 * x, 0, 0]));
    let tiles = prop::collection::vec(prop_oneof![3 => lower, 3 => upper, 1 => root], 0..5);
    (leaves, tiles)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// The union covers what either input covers and no more, keeps every leaf, holds no
    /// tile over finer content of either, is the same whichever input comes first, and takes
    /// each input's field without a merge (issue 8).
    #[test]
    fn the_union_covers_what_either_covers_the_finer_kept(a in shape(), b in shape()) {
        let (a, b) = (build(&a.0, &a.1), build(&b.0, &b.1));
        let (ab, ba) = (a.union(&b).unwrap(), b.union(&a).unwrap());
        is_the_union(&a, &b, &ab)?;
        let (m1, m2) = (Model::of(&ab), Model::of(&ba));
        prop_assert_eq!((m1.leaves, m1.tiles), (m2.leaves, m2.tiles));
    }
}

/// A root tile holding the other's leaf is split down to it, the rest of each level tiles
/// one level down: 32³ − 1 upper tiles, 16³ − 1 lower tiles and the leaf.
#[test]
fn a_root_tile_over_a_leaf_is_split_down_to_it() {
    let (a, b) = (build(&[], &[(TileLevel::Root, [0; 3])]), build(&[[8, 16, 24]], &[]));
    let u = a.union(&b).unwrap();
    let m = Model::of(&u);
    let count = |l: TileLevel| m.tiles.iter().filter(|(t, _)| *t == l).count();
    assert_eq!(m.leaves, BTreeSet::from([[8, 16, 24]]));
    assert_eq!(
        [count(TileLevel::Root), count(TileLevel::Upper), count(TileLevel::Lower)],
        [0, 32 * 32 * 32 - 1, 16 * 16 * 16 - 1]
    );
    let empty = Topology::new();
    let alone = a.union(&empty).unwrap();
    assert_eq!(Model::of(&alone).tiles, BTreeSet::from([(TileLevel::Root, [0; 3])]));
    assert_eq!(
        empty.union(&empty).unwrap().leaf_count() + empty.union(&empty).unwrap().tile_count(),
        0
    );
}
