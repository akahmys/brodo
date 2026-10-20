//! Reference tier (TESTING.md §1): a gather equals the field read voxel by voxel. For every
//! leaf of random topologies, each of the 10³ gathered values, and of the 10³, 12³ or 14³
//! of a gather one, two or three layers wide (ADR-0015), must have the bits of
//! `Field::value_at` at that voxel, which finds it through the tree rather than the leaf
//! manifest; one layer wide, `gather_wide` gives `gather`'s values.

use brodo::field::Field;
use brodo::gather::{GATHER_VOXELS, GhostWidth, gather, gather_wide};
use brodo::topology::Topology;
use brodo::tree::{Coord, TileLevel};
use proptest::prelude::*;

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

/// Mostly activations around a few shared coordinates, so leaves meet leaves and tiles.
fn ops() -> impl Strategy<Value = Vec<Op>> {
    let kind = prop_oneof![6 => Just(None), 2 => level().prop_map(Some)];
    let edit = (kind, any::<bool>(), 0..4_usize, [0..24_i32, 0..24, 0..24]);
    (prop::collection::vec(coord(), 4), prop::collection::vec(edit, 1..40)).prop_map(
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

/// Distinct values everywhere: each leaf voxel and each tile its own number.
fn field(t: &Topology) -> Field<u32> {
    let mut f = Field::new(t, u32::MAX);
    let mut k = 0;
    for n in t.leaf_numbers() {
        for v in f.leaf_mut(t, n).unwrap() {
            *v = k;
            k += 1;
        }
    }
    for n in t.tile_numbers() {
        f.set_tile(t, n, k).unwrap();
        k += 1;
    }
    f
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn a_gather_equals_the_field_voxel_by_voxel(ops in ops()) {
        let t = build(&ops);
        let f = field(&t);
        let mut g = [0; GATHER_VOXELS];
        for n in t.leaf_numbers() {
            gather(&t, &f, n, &mut g).unwrap();
            let o = t.leaf_origin(n).unwrap();
            for (i, got) in g.iter().enumerate() {
                let d = [i % 10, i / 10 % 10, i / 100].map(|c| i32::try_from(c).unwrap() - 1);
                let Some(v) = [0, 1, 2].map(|k| o[k].checked_add(d[k])).iter().copied().collect::<Option<Vec<i32>>>()
                else { continue };
                let v = [v[0], v[1], v[2]];
                prop_assert_eq!(*got, f.value_at(&t, v).unwrap(), "leaf at {:?}, voxel {:?}", o, v);
            }
        }
    }

    #[test]
    fn a_wide_gather_equals_the_field_voxel_by_voxel(
        ops in ops(),
        width in prop_oneof![Just(GhostWidth::One), Just(GhostWidth::Two), Just(GhostWidth::Three)],
    ) {
        let t = build(&ops);
        let f = field(&t);
        let (w, e) = (width.layers(), width.edge());
        let mut g = vec![7; 3];
        let mut one = [0; GATHER_VOXELS];
        for n in t.leaf_numbers() {
            gather_wide(&t, &f, n, width, &mut g).unwrap();
            prop_assert_eq!(g.len(), width.voxels());
            let o = t.leaf_origin(n).unwrap();
            for (i, got) in g.iter().enumerate() {
                let d = [i % e, i / e % e, i / (e * e)].map(|c| i32::try_from(c).unwrap() - w);
                let Some(v) = [0, 1, 2].map(|k| o[k].checked_add(d[k])).iter().copied().collect::<Option<Vec<i32>>>()
                else { continue };
                let v = [v[0], v[1], v[2]];
                prop_assert_eq!(*got, f.value_at(&t, v).unwrap(), "leaf at {:?}, voxel {:?}", o, v);
            }
            if width == GhostWidth::One {
                gather(&t, &f, n, &mut one).unwrap();
                prop_assert_eq!(&g[..], &one[..]);
            }
        }
    }
}
