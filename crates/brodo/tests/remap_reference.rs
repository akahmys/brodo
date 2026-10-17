//! Reference tier (TESTING.md §1): topology edits keep every field's values through
//! `Remap`. A model written here holds the structure as plain sets and each field as plain
//! maps, and gives values by position (ADR-0014): a new leaf takes what its region held
//! before (its leaf, its tile's value, or the background); a new tile takes the old tile
//! that held it, the background where nothing was, or the merge rule applied to what its
//! region held before, the commit refused if the rule refuses. After every commit, three
//! fields must hold the same bits as their models, leaves and tiles must be numbered in
//! Morton order, and the fields not yet remapped must be refused.

use std::collections::{BTreeMap, BTreeSet};

use brodo::error::FieldError;
use brodo::field::{Field, LEAF_VOXELS, Value, uniform};
use brodo::topology::Topology;
use brodo::tree::{Coord, TileLevel};
use proptest::prelude::*;

const LEVELS: [TileLevel; 3] = [TileLevel::Root, TileLevel::Upper, TileLevel::Lower];

trait Val: Value + std::fmt::Debug {
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

/// The leaf-sized regions of the lower node's region at `o`.
fn regions(o: Coord) -> impl Iterator<Item = Coord> {
    (0..4096).map(move |i| [o[0] + i % 16 * 8, o[1] + i / 16 % 16 * 8, o[2] + i / 256 * 8])
}

/// The structure: leaves by origin, tiles by level and origin.
#[derive(Clone, Default)]
struct Shape {
    leaves: BTreeSet<Coord>,
    tiles: BTreeSet<(TileLevel, Coord)>,
}

impl Shape {
    fn tile_at(&self, v: Coord) -> Option<(TileLevel, Coord)> {
        LEVELS.into_iter().map(|l| (l, align(v, l.edge_log2()))).find(|k| self.tiles.contains(k))
    }

    fn activate(&mut self, v: Coord) -> bool {
        if self.tile_at(v).is_none() {
            self.leaves.insert(align(v, 3));
        }
        true
    }

    fn add_tile(&mut self, level: TileLevel, v: Coord) -> bool {
        let (s, o) = (level.edge_log2(), align(v, level.edge_log2()));
        // What holds the region (a tile at its level or above), or what it holds (a leaf or
        // a smaller tile whose origin lies in it, found by a range on x).
        let x = o[0]..o[0] + (1 << s);
        let inside = |k: &Coord| align(*k, s) == o;
        let from = |x0: i32| [x0, i32::MIN, i32::MIN];
        let busy = LEVELS.iter().any(|l| {
            (l.edge_log2() >= s && self.tiles.contains(&(*l, align(v, l.edge_log2()))))
                || (l.edge_log2() < s
                    && self
                        .tiles
                        .range((*l, from(x.start))..(*l, from(x.end)))
                        .any(|(_, k)| inside(k)))
        }) || self.leaves.range(from(x.start)..from(x.end)).any(inside);
        if !busy {
            self.tiles.insert((level, o));
        }
        !busy
    }

    fn split(&mut self, v: Coord) -> bool {
        let Some(key) = self.tile_at(v) else { return false };
        self.tiles.remove(&key);
        for (parent, child) in
            [(TileLevel::Root, TileLevel::Upper), (TileLevel::Upper, TileLevel::Lower)]
        {
            if parent.edge_log2() > key.0.edge_log2() {
                continue;
            }
            let (o, c) = (align(v, parent.edge_log2()), child.edge_log2());
            let n = 1 << (parent.edge_log2() - c);
            for z in 0..n {
                for y in 0..n {
                    for x in 0..n {
                        let at = [o[0] + (x << c), o[1] + (y << c), o[2] + (z << c)];
                        if at != align(v, c) {
                            self.tiles.insert((child, at));
                        }
                    }
                }
            }
        }
        self.leaves.insert(align(v, 3));
        true
    }

    fn merge(&mut self, level: TileLevel, v: Coord) -> bool {
        match level {
            TileLevel::Root => false,
            TileLevel::Lower => {
                let o = align(v, 3);
                let ok = self.tile_at(v).is_none() && self.leaves.remove(&o);
                if ok {
                    self.tiles.insert((TileLevel::Lower, o));
                }
                ok
            }
            TileLevel::Upper => {
                let o = align(v, 7);
                let covered =
                    matches!(self.tile_at(v), Some((TileLevel::Root | TileLevel::Upper, _)));
                let full = regions(o).all(|r| {
                    self.leaves.contains(&r) || self.tiles.contains(&(TileLevel::Lower, r))
                });
                if covered || !full {
                    return false;
                }
                for r in regions(o) {
                    self.leaves.remove(&r);
                    self.tiles.remove(&(TileLevel::Lower, r));
                }
                self.tiles.insert((TileLevel::Upper, o));
                true
            }
        }
    }
}

/// One field's values on a shape.
#[derive(Clone)]
struct Values<T> {
    leaves: BTreeMap<Coord, Vec<T>>,
    tiles: BTreeMap<(TileLevel, Coord), T>,
    background: T,
}

/// What a leaf-sized region held.
enum Held<'a, T> {
    Leaf(&'a [T]),
    Tile(T, TileLevel),
    Background,
}

impl<T: Val> Values<T> {
    fn held(&self, r: Coord) -> Held<'_, T> {
        if let Some(v) = self.leaves.get(&r) {
            return Held::Leaf(v);
        }
        LEVELS
            .into_iter()
            .find_map(|l| self.tiles.get(&(l, align(r, l.edge_log2()))).map(|x| Held::Tile(*x, l)))
            .unwrap_or(Held::Background)
    }

    /// The values on `shape` after a commit, by position; `None` if the rule refuses.
    fn after(&self, shape: &Shape) -> Option<Values<T>> {
        let leaves = shape
            .leaves
            .iter()
            .map(|r| {
                let v = match self.held(*r) {
                    Held::Leaf(v) => v.to_vec(),
                    Held::Tile(x, _) => vec![x; LEAF_VOXELS],
                    Held::Background => vec![self.background; LEAF_VOXELS],
                };
                (*r, v)
            })
            .collect();
        let mut tiles = BTreeMap::new();
        for (level, o) in &shape.tiles {
            tiles.insert((*level, *o), self.tile_after(*level, *o)?);
        }
        Some(Values { leaves, tiles, background: self.background })
    }

    fn tile_after(&self, level: TileLevel, o: Coord) -> Option<T> {
        if let Held::Tile(x, l) = self.held(o)
            && l.edge_log2() >= level.edge_log2()
        {
            return Some(x);
        }
        let covered: Vec<Coord> = match level {
            TileLevel::Upper => regions(o).collect(),
            TileLevel::Lower | TileLevel::Root => vec![o],
        };
        let mut list = Vec::new();
        for r in covered {
            match self.held(r) {
                Held::Leaf(v) => list.extend(v.iter().map(|x| (*x, 1))),
                Held::Tile(x, _) => list.push((x, 512)),
                Held::Background => {}
            }
        }
        if list.is_empty() {
            return Some(self.background);
        }
        if level == TileLevel::Upper && list.iter().map(|(_, c)| c).sum::<u32>() < 4096 * 512 {
            list.push((self.background, 1));
        }
        uniform(&list)
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

#[derive(Debug, Clone, Copy)]
enum Op {
    Activate(Coord),
    AddTile(TileLevel, Coord),
    Split(Coord),
    Merge(TileLevel, Coord),
    /// Every leaf-sized region of a lower node's region made a tile.
    FillTiles(Coord),
}

fn coord() -> impl Strategy<Value = Coord> {
    let anchors = vec![-4097, -4096, -129, -128, -9, -8, -1, 0, 7, 8, 127, 128, 4095, 4096];
    let axis = (prop::sample::select(anchors), 0..3_i32).prop_map(|(a, d)| a + d);
    [axis.clone(), axis.clone(), axis]
}

/// A tile level. No root tiles: splitting one makes 36 862 tiles, which every later
/// commit would carry through the model; the tree's tests and `field`'s split a root tile.
fn level() -> impl Strategy<Value = TileLevel> {
    prop_oneof![Just(TileLevel::Upper), Just(TileLevel::Lower)]
}

/// Commits of edits on a few shared coordinates, so most splits and merges find what they
/// need; each with a seed for the values written after it.
fn commits() -> impl Strategy<Value = Vec<(Vec<Op>, u64)>> {
    let kind = prop_oneof![
        6 => Just(0_u8),
        4 => Just(1),
        4 => Just(2),
        3 => Just(3),
        2 => Just(4),
        1 => Just(5),
        2 => Just(6),
    ];
    let edit = (kind, level(), 0..6_usize, [0..2_i32, 0..2, 0..2]);
    let commit = (prop::collection::vec(edit, 1..6), any::<u64>());
    let ops = |pool: &[Coord], edits: Vec<(u8, TileLevel, usize, [i32; 3])>| -> Vec<Op> {
        edits
            .into_iter()
            .flat_map(|(kind, l, i, d)| {
                let v = [0, 1, 2].map(|k| pool[i][k] + d[k]);
                match kind {
                    0 => vec![Op::Activate(v)],
                    1 => vec![Op::AddTile(l, v)],
                    2 => vec![Op::Split(v)],
                    3 => vec![Op::Merge(TileLevel::Lower, v)],
                    4 => vec![Op::Merge(TileLevel::Upper, v)],
                    5 => vec![Op::FillTiles(v)],
                    // A lower node's region of tiles, two split into leaves, merged into an
                    // upper-level tile in the same commit.
                    _ => {
                        let o = align(v, 7);
                        let at = |k: i32| [o[0] + 8 * k, o[1] + 40, o[2] + 64];
                        vec![
                            Op::FillTiles(v),
                            Op::Split(at(1)),
                            Op::Split(at(9)),
                            Op::Merge(TileLevel::Upper, v),
                        ]
                    }
                }
            })
            .collect()
    };
    (prop::collection::vec(coord(), 6), prop::collection::vec(commit, 1..6)).prop_map(
        move |(pool, cs)| cs.into_iter().map(|(edits, seed)| (ops(&pool, edits), seed)).collect(),
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
    m: Values<T>,
}

impl<T: Val> Pair<T> {
    fn new(t: &Topology, background: T) -> Self {
        let m = Values { leaves: BTreeMap::new(), tiles: BTreeMap::new(), background };
        Self { f: Field::new(t, background), m }
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

    /// Writes values; one round in three writes one value everywhere, so that merges are
    /// accepted as well as refused.
    fn write(&mut self, t: &Topology, seed: u64) {
        let mut g = Lcg(seed);
        let everywhere = g.next().is_multiple_of(3).then(|| g.next() % 8);
        for n in t.leaf_numbers() {
            let vals: Vec<T> = (0..LEAF_VOXELS)
                .map(|_| T::from_k(everywhere.unwrap_or_else(|| g.next() % 8)))
                .collect();
            self.f.leaf_mut(t, n).unwrap().copy_from_slice(&vals);
            self.m.leaves.insert(t.leaf_origin(n).unwrap(), vals);
        }
        for n in t.tile_numbers() {
            let v = T::from_k(everywhere.unwrap_or_else(|| g.next() % 8));
            self.f.set_tile(t, n, v).unwrap();
            self.m.tiles.insert(t.tile(n).unwrap(), v);
        }
    }

    /// Remaps the field and its model; whether the rule accepted, after checking both
    /// agree on it.
    fn remap(&mut self, r: &brodo::topology::Remap, shape: &Shape) -> Result<bool, TestCaseError> {
        let got = self.f.remap(r, &uniform);
        let want = self.m.after(shape);
        prop_assert_eq!(
            got.is_ok(),
            want.is_some(),
            "rule refused in one only: {:?}",
            got.as_ref().err()
        );
        if let (Ok(f), Some(m)) = (got, want) {
            (self.f, self.m) = (f, m);
            return Ok(true);
        }
        Ok(false)
    }
}

fn check_order(t: &Topology) -> Result<(), TestCaseError> {
    let leaves: Vec<Coord> = t.leaf_numbers().map(|n| t.leaf_origin(n).unwrap()).collect();
    prop_assert!(leaves.windows(2).all(|w| morton_less(w[0], w[1])), "leaves out of Morton order");
    let tiles: Vec<Coord> = t.tile_numbers().map(|n| t.tile(n).unwrap().1).collect();
    prop_assert!(tiles.windows(2).all(|w| morton_less(w[0], w[1])), "tiles out of Morton order");
    Ok(())
}

/// Applies one edit to the editor and the shape; whether both accepted, after checking they
/// agree.
fn edit(
    e: &mut brodo::topology::TopologyEditor<'_>,
    s: &mut Shape,
    op: Op,
) -> Result<(), TestCaseError> {
    let (got, want) = match op {
        Op::Activate(v) => (e.activate(v).is_ok(), s.activate(v)),
        Op::AddTile(l, v) => (e.add_tile(l, v).is_ok(), s.add_tile(l, v)),
        Op::Split(v) => (e.split_tile(v).is_ok(), s.split(v)),
        Op::Merge(l, v) => (e.merge(l, v).is_ok(), s.merge(l, v)),
        Op::FillTiles(v) => {
            for r in regions(align(v, 7)) {
                prop_assert_eq!(
                    e.add_tile(TileLevel::Lower, r).is_ok(),
                    s.add_tile(TileLevel::Lower, r)
                );
            }
            (true, true)
        }
    };
    prop_assert_eq!(got, want, "{:?}", op);
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn remap_keeps_every_fields_values(commits in commits()) {
        let mut t = Topology::new();
        let mut shape = Shape::default();
        // The same k for every background too, so a merge's empty regions agree or disagree
        // with its other values alike in all three fields.
        let mut a = Pair::new(&t, f32::from_k(1));
        let mut b = Pair::new(&t, <[f32; 3]>::from_k(1));
        let mut c = Pair::new(&t, u8::from_k(1));
        for (ops, seed) in commits {
            let mut e = t.edit();
            let mut next = shape.clone();
            for op in ops {
                edit(&mut e, &mut next, op)?;
            }
            let (t2, r) = e.commit().unwrap();
            prop_assert_eq!(a.f.value_at(&t2, [0, 0, 0]), Err(FieldError::StaleField));
            // The same seed for all three, so each rule accepts or refuses the same tiles.
            let accepted = [a.remap(&r, &next)?, b.remap(&r, &next)?, c.remap(&r, &next)?];
            prop_assert!(accepted.iter().all(|x| *x == accepted[0]));
            if accepted[0] {
                (t, shape) = (t2, next);
                check_order(&t)?;
                a.agrees(&t)?;
                b.agrees(&t)?;
                c.agrees(&t)?;
            }
            a.write(&t, seed);
            b.write(&t, seed);
            c.write(&t, seed);
        }
    }
}
