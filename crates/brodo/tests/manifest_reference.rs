//! Reference tier (TESTING.md §1): every leaf's 26 neighbours match those computed from
//! coordinates. The expected neighbour is found from the topology's lists of leaf origins
//! and tiles alone, without the tree: a leaf whose origin is one leaf away, else a tile
//! whose region holds that voxel, else the background. Also: neighbours are symmetric.

use std::collections::BTreeMap;

use brodo::manifest::Offset;
use brodo::topology::{LeafNumber, Location, TileNumber, Topology};
use brodo::tree::{Coord, TileLevel};
use proptest::prelude::*;

const LEVELS: [TileLevel; 3] = [TileLevel::Root, TileLevel::Upper, TileLevel::Lower];

fn align(v: Coord, log2: u32) -> Coord {
    v.map(|c| c.div_euclid(1 << log2) * (1 << log2))
}

/// The neighbours computed from coordinates.
struct Expected {
    leaves: BTreeMap<Coord, LeafNumber>,
    tiles: BTreeMap<(TileLevel, Coord), TileNumber>,
}

impl Expected {
    fn new(t: &Topology) -> Self {
        let leaves = t.leaf_numbers().map(|n| (t.leaf_origin(n).unwrap(), n)).collect();
        let tiles = t.tile_numbers().map(|n| (t.tile(n).unwrap(), n)).collect();
        Self { leaves, tiles }
    }

    fn at(&self, origin: Coord, d: [i32; 3]) -> Location {
        let mut c = origin;
        for (x, k) in c.iter_mut().zip(d) {
            let Some(y) = x.checked_add(8 * k) else { return Location::Background };
            *x = y;
        }
        if let Some(n) = self.leaves.get(&c) {
            return Location::Leaf(*n);
        }
        LEVELS
            .into_iter()
            .find_map(|l| {
                self.tiles.get(&(l, align(c, l.edge_log2()))).map(|n| Location::Tile(*n, l))
            })
            .unwrap_or(Location::Background)
    }
}

#[derive(Debug, Clone)]
enum Op {
    Activate(Coord),
    AddTile(TileLevel, Coord),
    Split(Coord),
}

fn coord() -> impl Strategy<Value = Coord> {
    let anchors = vec![-4097, -4096, -129, -128, -9, -8, -1, 0, 7, 8, 127, 128, 4095, 4096];
    let axis = (prop::sample::select(anchors), 0..20_i32).prop_map(|(a, d)| a + d);
    [axis.clone(), axis.clone(), axis]
}

/// Root tiles are rarer: splitting one makes 36 862 tiles.
fn level() -> impl Strategy<Value = TileLevel> {
    prop_oneof![1 => Just(TileLevel::Root), 3 => Just(TileLevel::Upper), 3 => Just(TileLevel::Lower)]
}

/// Mostly activations around a few shared coordinates, so leaves cluster and meet.
fn ops() -> impl Strategy<Value = Vec<Op>> {
    let kind = prop_oneof![6 => Just(None), 2 => level().prop_map(Some)];
    let edit = (kind, any::<bool>(), 0..4_usize, [0..24_i32, 0..24, 0..24]);
    (prop::collection::vec(coord(), 4), prop::collection::vec(edit, 1..60)).prop_map(
        |(pool, edits)| {
            edits
                .into_iter()
                .map(|(kind, split, i, d)| {
                    let v = [0, 1, 2].map(|k| pool[i][k] + d[k]);
                    match (kind, split) {
                        (Some(l), _) => Op::AddTile(l, v),
                        (None, true) if d[0] % 4 == 0 => Op::Split(v),
                        (None, _) => Op::Activate(v),
                    }
                })
                .collect()
        },
    )
}

fn build(ops: &[Op]) -> Topology {
    let empty = Topology::new();
    let mut e = empty.edit();
    for op in ops {
        // Refused edits are skipped: the tree's own tests check refusals.
        let _refused = match *op {
            Op::Activate(v) => e.activate(v).map(|_| ()),
            Op::AddTile(l, v) => e.add_tile(l, v),
            Op::Split(v) => e.split_tile(v),
        };
    }
    e.commit().unwrap().0
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn neighbours_match_coordinates_and_are_symmetric(ops in ops()) {
        let t = build(&ops);
        let want = Expected::new(&t);
        for n in t.leaf_numbers() {
            let origin = t.leaf_origin(n).unwrap();
            let row = t.neighbours(n).unwrap();
            for d in Offset::all() {
                let got = row[d.index()];
                prop_assert_eq!(got, want.at(origin, d.delta()), "leaf {:?} towards {:?}", origin, d.delta());
                if let Location::Leaf(m) = got {
                    prop_assert_eq!(t.manifest().neighbour(m, d.opposite()).unwrap(), Location::Leaf(n));
                }
            }
        }
    }
}
