//! Reference tier (TESTING.md §1): a field moved onto an unrelated topology by
//! `Remap::between` and `Field::remap` (ADR-0014; issue 7). A model written here holds each
//! topology as plain sets of leaf origins and tiles, read from their listings, and gives
//! each new voxel what the old one held: a new leaf takes, voxel by voxel, the old leaf's
//! value, the old tile's or the background; a new tile takes the old tile covering it, the
//! background where the old held nothing in its region, or the merge rule applied to what
//! each leaf-sized region of it held, its empty regions counted together; a new root tile
//! over anything but one old root tile or nothing is refused.

use std::collections::BTreeSet;

use brodo::error::{FieldError, TreeError};
use brodo::field::{Field, uniform};
use brodo::topology::{Remap, Topology};
use brodo::tree::{Coord, TileLevel};
use proptest::prelude::*;

const LEVELS: [TileLevel; 3] = [TileLevel::Root, TileLevel::Upper, TileLevel::Lower];
const BACKGROUND: u32 = 7;

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

/// What a topology holds, read from its listings alone.
struct Model {
    leaves: BTreeSet<Coord>,
    tiles: BTreeSet<(TileLevel, Coord)>,
}

/// What holds a voxel in the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Held {
    Leaf(Coord),
    Tile(TileLevel, Coord),
    Background,
}

impl Model {
    fn of(t: &Topology) -> Self {
        Self {
            leaves: t.leaf_numbers().map(|n| t.leaf_origin(n).unwrap()).collect(),
            tiles: t.tile_numbers().map(|n| t.tile(n).unwrap()).collect(),
        }
    }

    fn held(&self, v: Coord) -> Held {
        if self.leaves.contains(&align(v, 3)) {
            return Held::Leaf(align(v, 3));
        }
        LEVELS
            .into_iter()
            .map(|l| (l, align(v, l.edge_log2())))
            .find(|k| self.tiles.contains(k))
            .map_or(Held::Background, |(l, o)| Held::Tile(l, o))
    }

    /// Whether anything lies in the region of edge `edge` at `o`.
    fn holds_in(&self, o: Coord, edge: i32) -> bool {
        let within = |c: &Coord| (0..3).all(|k| c[k] >= o[k] && c[k] < o[k] + edge);
        self.leaves.iter().any(within)
            || self.tiles.iter().any(|(l, c)| {
                let e = l.edge();
                (0..3).all(|k| c[k] < o[k] + edge && o[k] < c[k] + e)
            })
    }
}

/// The value of a numbered field at `v`: a leaf voxel its leaf's place and the voxel's, a
/// tile its tile's place, else the background.
fn value(t: &Topology, f: &Field<u32>, v: Coord) -> u32 {
    f.value_at(t, v).unwrap()
}

/// A field whose every leaf voxel and tile holds its own value.
fn numbered(t: &Topology) -> Field<u32> {
    let mut f = Field::new(t, BACKGROUND);
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

/// A rule giving each value times its count, summed, which the model computes alike.
fn summed(parts: &[(u32, u32)]) -> Option<u32> {
    Some(parts.iter().fold(0_u32, |s, (v, c)| s.wrapping_add(v.wrapping_mul(*c))))
}

/// What the model gives a new tile at `level` and `o`: `None` where it is refused.
fn tile_expected(
    m: &Model,
    t: &Topology,
    f: &Field<u32>,
    level: TileLevel,
    o: Coord,
) -> Option<u32> {
    let edge = level.edge();
    if let Held::Tile(l, c) = m.held(o)
        && l.edge() >= edge
    {
        return Some(value(t, f, c));
    }
    if !m.holds_in(o, edge) {
        return Some(BACKGROUND);
    }
    if level == TileLevel::Root {
        return None;
    }
    let side = edge / 8;
    let mut sum = 0_u32;
    for i in 0..side * side * side {
        let r = [o[0] + i % side * 8, o[1] + i / side % side * 8, o[2] + i / (side * side) * 8];
        let add = |v: u32, c: u32| v.wrapping_mul(c);
        sum = sum.wrapping_add(match m.held(r) {
            Held::Leaf(_) => (0..512)
                .map(|k| {
                    let v = [r[0] + k % 8, r[1] + k / 8 % 8, r[2] + k / 64];
                    value(t, f, v)
                })
                .fold(0_u32, u32::wrapping_add),
            Held::Tile(..) | Held::Background => add(value(t, f, r), 512),
        });
    }
    Some(sum)
}

/// Fails unless `f`, moved from `from` onto `to`, holds what the model gives at every
/// voxel of every new leaf and at every new tile.
fn moved_as_the_model_gives(from: &Topology, to: &Topology) -> Result<(), TestCaseError> {
    let f = numbered(from);
    let m = Model::of(from);
    let r = Remap::between(from, to);
    let expected: Option<Vec<u32>> = to
        .tile_numbers()
        .map(|n| {
            let (l, o) = to.tile(n).unwrap();
            tile_expected(&m, from, &f, l, o)
        })
        .collect();
    let Some(expected) = expected else {
        prop_assert_eq!(r.err(), Some(TreeError::MergeUnsupported(TileLevel::Root)));
        return Ok(());
    };
    let g = f.remap(&r.unwrap(), &summed).unwrap();
    for n in to.leaf_numbers() {
        for v in to.leaf_voxels(n).unwrap() {
            prop_assert_eq!(value(to, &g, v), value(from, &f, v), "voxel {:?}", v);
        }
    }
    for (n, want) in to.tile_numbers().zip(expected) {
        let (_, o) = to.tile(n).unwrap();
        prop_assert_eq!(value(to, &g, o), want, "tile {:?}", to.tile(n).unwrap());
    }
    prop_assert_eq!(g.background(), BACKGROUND);
    Ok(())
}

/// Leaves, lower tiles and upper tiles near the origin, and root tiles a root region
/// away, so that two drawn topologies meet in every way the model names.
fn shape() -> impl Strategy<Value = (Vec<Coord>, Vec<(TileLevel, Coord)>)> {
    let near = || (-2..3_i32, -2..3_i32, -2..3_i32);
    let leaves = prop::collection::vec(near().prop_map(|(x, y, z)| [8 * x, 8 * y, 8 * z]), 0..12);
    let lower = near().prop_map(|(x, y, z)| (TileLevel::Lower, [8 * x, 8 * y, 8 * z]));
    let upper = (-1..1_i32, -1..1_i32, -1..1_i32)
        .prop_map(|(x, y, z)| (TileLevel::Upper, [128 * x, 128 * y, 128 * z]));
    let root = (0..2_i32).prop_map(|x| (TileLevel::Root, [4096 * (x + 1), 0, 0]));
    let tiles = prop::collection::vec(prop_oneof![lower, upper, root], 0..6);
    (leaves, tiles)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Every new leaf voxel holds what the old topology held there, and every new tile what
    /// the model gives (issue 7).
    #[test]
    fn a_field_moves_onto_another_topology_as_the_model_gives(
        a in shape(),
        b in shape(),
    ) {
        let (from, to) = (build(&a.0, &a.1), build(&b.0, &b.1));
        moved_as_the_model_gives(&from, &to)?;
        moved_as_the_model_gives(&to, &from)?;
    }
}

/// A field moved onto its own topology keeps every value's bits, brodo's rule refusing no
/// tile: each leaf and tile is its own source.
#[test]
fn a_field_moved_onto_its_own_topology_is_unchanged() {
    let t = build(
        &[[-8, 0, 0], [0, 0, 0], [16, 8, 0], [130, 0, 0]],
        &[
            (TileLevel::Lower, [8, 8, 8]),
            (TileLevel::Upper, [128, 128, 0]),
            (TileLevel::Root, [4096, 0, 0]),
        ],
    );
    let f = numbered(&t);
    let g = f.remap(&Remap::between(&t, &t).unwrap(), &uniform).unwrap();
    for n in t.leaf_numbers() {
        assert_eq!(g.leaf(&t, n).unwrap(), f.leaf(&t, n).unwrap());
    }
    for n in t.tile_numbers() {
        let (_, o) = t.tile(n).unwrap();
        assert_eq!(value(&t, &g, o), value(&t, &f, o));
    }
}

/// A new root tile takes the old root tile there, or the background where the old held
/// nothing in its region; over anything else it is refused, listing its parts by
/// leaf-sized region being left out (ADR-0014).
#[test]
fn a_root_tile_over_what_is_not_one_root_tile_or_nothing_is_refused() {
    let root = build(&[], &[(TileLevel::Root, [0; 3])]);
    for from in [
        build(&[[8, 0, 0]], &[]),
        build(&[], &[(TileLevel::Upper, [128, 0, 0])]),
        build(&[], &[(TileLevel::Lower, [8, 8, 8])]),
    ] {
        assert_eq!(
            Remap::between(&from, &root).err(),
            Some(TreeError::MergeUnsupported(TileLevel::Root))
        );
    }
    let elsewhere = build(&[[-8, 0, 0]], &[(TileLevel::Root, [4096, 0, 0])]);
    let g =
        numbered(&elsewhere).remap(&Remap::between(&elsewhere, &root).unwrap(), &uniform).unwrap();
    assert_eq!(value(&root, &g, [5, 5, 5]), BACKGROUND);
    let same = build(&[[-8, 0, 0]], &[(TileLevel::Root, [0; 3])]);
    let g = numbered(&same).remap(&Remap::between(&same, &root).unwrap(), &uniform).unwrap();
    assert_eq!(value(&root, &g, [5, 5, 5]), value(&same, &numbered(&same), [5, 5, 5]));
}

/// A field of another topology than the remap's source is refused, and the field made is
/// the new topology's.
#[test]
fn a_field_of_another_topology_is_refused_and_the_new_one_is_the_targets() {
    let (a, b) = (build(&[[0; 3]], &[]), build(&[[8, 0, 0]], &[]));
    let r = Remap::between(&a, &b).unwrap();
    assert!(matches!(numbered(&b).remap(&r, &uniform), Err(FieldError::StaleField)));
    let g = numbered(&a).remap(&r, &uniform).unwrap();
    assert!(g.value_at(&b, [8, 0, 0]).is_ok());
    assert!(matches!(g.value_at(&a, [0, 0, 0]), Err(FieldError::StaleField)));
}
