//! Reference tier (TESTING.md §1): random activation and writes give the same values as a
//! dense grid. The dense grid is a plain array over a box of 24³ voxels around the origin.
//! A write to a leaf voxel sets one element; a write to a tile sets every element its region
//! covers; a topology edit changes no value at all. After every commit and every round of
//! writes, every voxel of the box must read the same bits from the grid.

use brodo::grid::Grid;
use brodo::topology::Location;
use brodo::tree::{Coord, TileLevel};
use proptest::prelude::*;

/// The box: voxels from LO to HI - 1 on each axis.
const LO: i32 = -12;
const HI: i32 = 12;
const SIDE: usize = 24;
const BACKGROUND: f32 = 0.25;

/// The dense grid.
struct Dense(Vec<f32>);

impl Dense {
    fn new() -> Self {
        Self(vec![BACKGROUND; SIDE * SIDE * SIDE])
    }

    fn index(v: Coord) -> usize {
        let [x, y, z] = v.map(|c| usize::try_from(c - LO).unwrap());
        x + SIDE * (y + SIDE * z)
    }

    fn voxels() -> impl Iterator<Item = Coord> {
        (LO..HI).flat_map(|z| (LO..HI).flat_map(move |y| (LO..HI).map(move |x| [x, y, z])))
    }

    /// Sets every voxel of the box within the aligned cube of edge `2^log2` holding `v`.
    fn fill(&mut self, level: TileLevel, v: Coord, value: f32) {
        let s = 1 << level.edge_log2();
        let o = v.map(|c| c.div_euclid(s) * s);
        for w in Self::voxels() {
            if (0..3).all(|k| (o[k]..o[k] + s).contains(&w[k])) {
                self.0[Self::index(w)] = value;
            }
        }
    }
}

#[derive(Debug, Clone)]
enum Op {
    Activate(Coord),
    AddTile(TileLevel, Coord),
    Split(Coord),
}

fn voxel() -> impl Strategy<Value = Coord> {
    [LO..HI, LO..HI, LO..HI]
}

/// Root tiles are rarer: splitting one makes 36 862 tiles.
fn level() -> impl Strategy<Value = TileLevel> {
    prop_oneof![1 => Just(TileLevel::Root), 2 => Just(TileLevel::Upper), 6 => Just(TileLevel::Lower)]
}

/// Edits, each committed, then writes at voxels with values.
type Round = (Vec<Op>, Vec<(Coord, i16)>);

/// Rounds on a few shared voxels, each moved by up to two voxels, so that most splits find
/// a tile and most writes find a leaf or a tile.
fn rounds() -> impl Strategy<Value = Vec<Round>> {
    let near = (0..6_usize, [0..3_i32, 0..3, 0..3]);
    let kind = prop_oneof![5 => Just(None), 2 => level().prop_map(Some)];
    let edit = (kind, any::<bool>(), near.clone());
    let round =
        (prop::collection::vec(edit, 1..8), prop::collection::vec((near, any::<i16>()), 0..30));
    (prop::collection::vec(voxel(), 6), prop::collection::vec(round, 1..5)).prop_map(
        |(pool, rounds)| {
            let at = |(i, d): (usize, [i32; 3])| -> Coord {
                [0, 1, 2].map(|k| (pool[i][k] + d[k]).min(HI - 1))
            };
            rounds
                .into_iter()
                .map(|(edits, writes)| {
                    let ops = edits
                        .into_iter()
                        .map(|(kind, split, n)| match (kind, split) {
                            (Some(l), _) => Op::AddTile(l, at(n)),
                            (None, true) => Op::Split(at(n)),
                            (None, false) => Op::Activate(at(n)),
                        })
                        .collect();
                    (ops, writes.into_iter().map(|(n, x)| (at(n), x)).collect())
                })
                .collect()
        },
    )
}

fn check(g: &Grid<f32>, d: &Dense) -> Result<(), TestCaseError> {
    for v in Dense::voxels() {
        let got = g.value_at(v).unwrap();
        prop_assert_eq!(got.to_bits(), d.0[Dense::index(v)].to_bits(), "at {:?}", v);
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn activation_and_writes_match_a_dense_grid(rounds in rounds()) {
        let mut g = Grid::new(BACKGROUND);
        let mut d = Dense::new();
        for (ops, writes) in rounds {
            for op in ops {
                // Each edit is committed alone, so a refused one leaves the rest in.
                let _refused = g.edit(|e| match op {
                    Op::Activate(v) => e.activate(v).map(|_| ()),
                    Op::AddTile(l, v) => e.add_tile(l, v),
                    Op::Split(v) => e.split_tile(v),
                });
            }
            check(&g, &d)?;
            for (v, x) in writes {
                let value = f32::from(x) / 8.0;
                let (t, f) = g.parts_mut();
                match t.locate(v) {
                    Location::Leaf(n) => {
                        let [a, b, c] = v.map(|k| usize::try_from(k.rem_euclid(8)).unwrap());
                        f.leaf_mut(t, n).unwrap()[a + 8 * b + 64 * c] = value;
                        d.0[Dense::index(v)] = value;
                    }
                    Location::Tile(n, level) => {
                        f.set_tile(t, n, value).unwrap();
                        d.fill(level, v, value);
                    }
                    Location::Background => {}
                }
            }
            check(&g, &d)?;
        }
    }
}
