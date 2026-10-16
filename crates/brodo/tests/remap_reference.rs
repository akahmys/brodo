//! Reference tier (TESTING.md §1): topology edits keep every field's values through
//! `Remap`. A model written here holds each field as plain maps, leaf values and tile values
//! by origin, and applies each edit to values directly. After every commit, three fields
//! must hold the same bits as their models, leaves and tiles must be numbered in Morton
//! order, and the fields not yet remapped must be refused.

use std::collections::BTreeMap;

use brodo::error::FieldError;
use brodo::field::{Field, LEAF_VOXELS};
use brodo::topology::Topology;
use brodo::tree::{Coord, TileLevel};
use proptest::prelude::*;

const LEVELS: [TileLevel; 3] = [TileLevel::Root, TileLevel::Upper, TileLevel::Lower];

trait Val: Copy + Send + Sync + 'static + std::fmt::Debug {
    fn from_k(k: u32) -> Self;
    fn bits(self) -> [u32; 3];
}
impl Val for f32 {
    fn from_k(k: u32) -> Self {
        k as f32 * 0.5
    }
    fn bits(self) -> [u32; 3] {
        [self.to_bits(), 0, 0]
    }
}
impl Val for [f32; 3] {
    fn from_k(k: u32) -> Self {
        [k as f32, -(k as f32), 0.25]
    }
    fn bits(self) -> [u32; 3] {
        self.map(f32::to_bits)
    }
}
impl Val for u8 {
    fn from_k(k: u32) -> Self {
        u8::try_from(k).unwrap()
    }
    fn bits(self) -> [u32; 3] {
        [u32::from(self), 0, 0]
    }
}

fn align(v: Coord, log2: u32) -> Coord {
    v.map(|c| c.div_euclid(1 << log2) * (1 << log2))
}

/// One field as plain maps: leaf values by origin, tile values by level and origin.
#[derive(Clone)]
struct Model<T> {
    leaves: BTreeMap<Coord, Vec<T>>,
    tiles: BTreeMap<(TileLevel, Coord), T>,
    background: T,
}

impl<T: Val> Model<T> {
    fn new(background: T) -> Self {
        Self { leaves: BTreeMap::new(), tiles: BTreeMap::new(), background }
    }

    fn tile_at(&self, v: Coord) -> Option<(TileLevel, Coord)> {
        LEVELS
            .into_iter()
            .map(|l| (l, align(v, l.edge_log2())))
            .find(|k| self.tiles.contains_key(k))
    }

    fn activate(&mut self, v: Coord) {
        if self.tile_at(v).is_none() {
            self.leaves.entry(align(v, 3)).or_insert_with(|| vec![self.background; LEAF_VOXELS]);
        }
    }

    fn add_tile(&mut self, level: TileLevel, v: Coord) -> bool {
        let (s, o) = (level.edge_log2(), align(v, level.edge_log2()));
        let meet = |s2: u32, o2: Coord| align(o, s.max(s2)) == align(o2, s.max(s2));
        let busy = self.leaves.keys().any(|k| meet(3, *k))
            || self.tiles.keys().any(|(l, k)| meet(l.edge_log2(), *k));
        if !busy {
            self.tiles.insert((level, o), self.background);
        }
        !busy
    }

    fn split(&mut self, v: Coord) -> bool {
        let Some(key) = self.tile_at(v) else { return false };
        let Some(value) = self.tiles.remove(&key) else { return false };
        for (parent, child) in
            [(TileLevel::Root, TileLevel::Upper), (TileLevel::Upper, TileLevel::Lower)]
        {
            if parent.edge_log2() > key.0.edge_log2() {
                continue;
            }
            let o = align(v, parent.edge_log2());
            let n = 1 << (parent.edge_log2() - child.edge_log2());
            let c = child.edge_log2();
            for z in 0..n {
                for y in 0..n {
                    for x in 0..n {
                        let at = [o[0] + (x << c), o[1] + (y << c), o[2] + (z << c)];
                        if at != align(v, c) {
                            self.tiles.insert((child, at), value);
                        }
                    }
                }
            }
        }
        self.leaves.insert(align(v, 3), vec![value; LEAF_VOXELS]);
        true
    }
}

/// The Morton key, computed bit by bit from the top: the order the topology must follow.
fn morton_less(a: Coord, b: Coord) -> bool {
    for bit in (0..32).rev() {
        for axis in (0..3).rev() {
            let x = |c: Coord| ((c[axis].cast_unsigned() ^ 0x8000_0000) >> bit) & 1;
            if x(a) != x(b) {
                return x(a) < x(b);
            }
        }
    }
    false
}

#[derive(Debug, Clone)]
enum Op {
    Activate(Coord),
    AddTile(TileLevel, Coord),
    Split(Coord),
}

fn coord() -> impl Strategy<Value = Coord> {
    let anchors = vec![-4097, -4096, -129, -128, -9, -8, -1, 0, 7, 8, 127, 128, 4095, 4096];
    let axis = (prop::sample::select(anchors), 0..3_i32).prop_map(|(a, d)| a + d);
    [axis.clone(), axis.clone(), axis]
}

/// A tile level; Root tiles are rarer, as splitting one makes 36 862 tiles, which the model
/// holds one by one.
fn level() -> impl Strategy<Value = TileLevel> {
    prop_oneof![1 => Just(TileLevel::Root), 3 => Just(TileLevel::Upper), 3 => Just(TileLevel::Lower)]
}

/// Commits of edits on a few shared coordinates, so most splits find a tile; each with a
/// seed for the values written after it.
fn commits() -> impl Strategy<Value = Vec<(Vec<Op>, u64)>> {
    let kind = prop_oneof![3 => Just(None), 2 => level().prop_map(Some)];
    let edit = (kind, any::<bool>(), 0..6_usize, [0..2_i32, 0..2, 0..2]);
    let commit = (prop::collection::vec(edit, 1..6), any::<u64>());
    (prop::collection::vec(coord(), 6), prop::collection::vec(commit, 1..6)).prop_map(
        |(pool, cs)| {
            cs.into_iter()
                .map(|(edits, seed)| {
                    let ops = edits
                        .into_iter()
                        .map(|(kind, split, i, d)| {
                            let v = [0, 1, 2].map(|k| pool[i][k] + d[k]);
                            match (kind, split) {
                                (Some(l), _) => Op::AddTile(l, v),
                                (None, true) => Op::Split(v),
                                (None, false) => Op::Activate(v),
                            }
                        })
                        .collect();
                    (ops, seed)
                })
                .collect()
        },
    )
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }
}

/// A field and its model.
struct Pair<T> {
    f: Field<T>,
    m: Model<T>,
}

impl<T: Val> Pair<T> {
    fn new(t: &Topology, background: T) -> Self {
        Self { f: Field::new(t, background), m: Model::new(background) }
    }

    fn agrees(&self, t: &Topology) -> Result<(), TestCaseError> {
        prop_assert_eq!(t.leaf_count(), self.m.leaves.len());
        prop_assert_eq!(t.tile_count(), self.m.tiles.len());
        for n in t.leaf_numbers() {
            let o = t.leaf_origin(n).unwrap();
            let got: Vec<[u32; 3]> = self.f.leaf(t, n).unwrap().iter().map(|v| v.bits()).collect();
            let want: Vec<[u32; 3]> = self.m.leaves[&o].iter().map(|v| v.bits()).collect();
            prop_assert!(got == want, "leaf at {:?}", o);
        }
        for n in t.tile_numbers() {
            let key = t.tile(n).unwrap();
            prop_assert_eq!(
                self.f.tile(t, n).unwrap().bits(),
                self.m.tiles[&key].bits(),
                "tile {:?}",
                key
            );
        }
        Ok(())
    }

    fn write(&mut self, t: &Topology, seed: u64) {
        let mut g = Lcg(seed);
        for n in t.leaf_numbers() {
            let vals: Vec<T> = (0..LEAF_VOXELS).map(|_| T::from_k(g.next() % 8)).collect();
            self.f.leaf_mut(t, n).unwrap().copy_from_slice(&vals);
            self.m.leaves.insert(t.leaf_origin(n).unwrap(), vals);
        }
        for n in t.tile_numbers() {
            let v = T::from_k(g.next() % 8);
            self.f.set_tile(t, n, v).unwrap();
            self.m.tiles.insert(t.tile(n).unwrap(), v);
        }
    }

    fn apply(&mut self, op: &Op) -> bool {
        match *op {
            Op::Activate(v) => {
                self.m.activate(v);
                true
            }
            Op::AddTile(l, v) => self.m.add_tile(l, v),
            Op::Split(v) => self.m.split(v),
        }
    }
}

fn check_order(t: &Topology) -> Result<(), TestCaseError> {
    let leaves: Vec<Coord> = t.leaf_numbers().map(|n| t.leaf_origin(n).unwrap()).collect();
    prop_assert!(leaves.windows(2).all(|w| morton_less(w[0], w[1])), "leaves out of Morton order");
    let tiles: Vec<Coord> = t.tile_numbers().map(|n| t.tile(n).unwrap().1).collect();
    prop_assert!(tiles.windows(2).all(|w| morton_less(w[0], w[1])), "tiles out of Morton order");
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn remap_keeps_every_fields_values(commits in commits()) {
        let mut t = Topology::new();
        let mut a = Pair::new(&t, 0.5_f32);
        let mut b = Pair::new(&t, [1.0_f32, 2.0, 3.0]);
        let mut c = Pair::new(&t, 7_u8);
        for (ops, seed) in commits {
            let mut e = t.edit();
            for op in &ops {
                let ok = match *op {
                    Op::Activate(v) => e.activate(v).is_ok(),
                    Op::AddTile(l, v) => e.add_tile(l, v).is_ok(),
                    Op::Split(v) => e.split_tile(v).is_ok(),
                };
                let modelled = [a.apply(op), b.apply(op), c.apply(op)];
                prop_assert_eq!([ok; 3], modelled, "{:?}", op);
            }
            let (t2, r) = e.commit().unwrap();
            prop_assert_eq!(a.f.value_at(&t2, [0, 0, 0]), Err(FieldError::StaleField));
            a.f = a.f.remap(&r).unwrap();
            b.f = b.f.remap(&r).unwrap();
            c.f = c.f.remap(&r).unwrap();
            t = t2;
            check_order(&t)?;
            a.agrees(&t)?;
            b.agrees(&t)?;
            c.agrees(&t)?;
            a.write(&t, seed);
            b.write(&t, seed ^ 1);
            c.write(&t, seed ^ 2);
        }
    }
}
