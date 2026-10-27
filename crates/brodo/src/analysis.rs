//! Topology analysis (ARCHITECTURE.md §4.5, ADR-0013): connected components of the voxels
//! whose value meets a condition, tiles and the background included, and flood fill.
//!
//! The units joined are leaf voxels, tiles, the empty places of lower nodes (8³) and upper
//! nodes (128³), the empty regions at the root, found on a grid compressed along each axis
//! around the root's keys, and the unbounded outside. Two units are joined when both meet
//! the condition and they touch: across a face (6-connectivity), or across a face, an edge
//! or a corner (26-connectivity). Touching regions are found by descending the levels with
//! an explicit stack: a pair of regions of one size, or a region against a uniform unit.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::{FieldError, TreeError};
use crate::field::{Field, Value, leaf_place};
use crate::topology::{Location, Topology};
use crate::tree::{Coord, Place, TileLevel, align, child_place};

/// Which neighbours touch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connectivity {
    /// Across a face: 6 neighbours.
    Six,
    /// Across a face, an edge or a corner: 26 neighbours.
    TwentySix,
}

impl Connectivity {
    fn offsets(self) -> Vec<[i32; 3]> {
        let all =
            (-1..=1_i32).flat_map(|z| (-1..=1).flat_map(move |y| (-1..=1).map(move |x| [x, y, z])));
        all.filter(|d| {
            let n: i32 = d.iter().map(|c: &i32| c.abs()).sum();
            n > 0 && (self == Connectivity::TwentySix || n == 1)
        })
        .collect()
    }
}

/// Why an analysis failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AnalysisError {
    /// The field does not belong to the topology.
    #[error(transparent)]
    Field(#[from] FieldError),
    /// The topology refused a lookup by number.
    #[error(transparent)]
    Tree(#[from] TreeError),
    /// The root's keys are spread so widely that the compressed root grid would be too
    /// large.
    #[error("the root's keys are too spread out to label")]
    TooSpread,
    /// More units than a `u32` numbers.
    #[error("too many voxels, tiles and empty regions to label")]
    TooLarge,
    /// A voxel no unit stands for: the labels and topology disagree.
    #[error("voxel {0:?} has no unit")]
    Unlabelled(Coord),
    /// The voxels meeting the condition include the background, so are unbounded and have
    /// no Euler characteristic (ADR-0024).
    #[error("the background meets the condition: the set is unbounded")]
    Unbounded,
}

/// A voxel's label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Label {
    /// The component, numbered in the order of its first unit.
    Component(u32),
    /// The voxel's value does not meet the condition.
    Excluded,
}

/// The largest compressed root grid, in cells.
const ROOT_CELLS: usize = 1 << 22;
/// log2 of a region's edge at each level: a root region, a lower node's, a leaf's, a voxel.
const LOG: [u32; 4] = [12, 7, 3, 0];

/// One axis of the compressed root grid: intervals of root-region numbers, in order; the
/// first and last are unbounded.
#[derive(Debug, Clone)]
struct Axis(Vec<(i64, i64)>);

impl Axis {
    fn new(keys: &BTreeSet<i64>) -> Self {
        let near: BTreeSet<i64> = keys.iter().flat_map(|k| [k - 1, *k, k + 1]).collect();
        let mut out = vec![];
        let mut next = i64::MIN;
        for v in near {
            if v > next {
                out.push((next, v - 1));
            }
            out.push((v, v));
            next = v + 1;
        }
        out.push((next, i64::MAX));
        Self(out)
    }

    fn find(&self, r: i64) -> usize {
        self.0.partition_point(|(_, hi)| *hi < r)
    }

    fn singleton(&self, i: usize) -> Option<i64> {
        self.0.get(i).and_then(|(lo, hi)| (lo == hi).then_some(*lo))
    }

    fn unbounded(&self, i: usize) -> bool {
        i == 0 || i + 1 >= self.0.len()
    }
}

/// The root grid: compressed axes and the root's keys.
#[derive(Debug, Clone)]
struct RootGrid {
    axes: [Axis; 3],
    keys: BTreeSet<Coord>,
}

impl RootGrid {
    fn new(t: &Topology) -> Result<Self, AnalysisError> {
        let keys: BTreeSet<Coord> = t.root_keys().collect();
        let axis = |k: usize| Axis::new(&keys.iter().map(|c| i64::from(c[k] >> LOG[0])).collect());
        let axes = [axis(0), axis(1), axis(2)];
        let cells = axes.iter().try_fold(1_usize, |n, a| n.checked_mul(a.0.len()));
        if cells.is_none_or(|n| n > ROOT_CELLS) {
            return Err(AnalysisError::TooSpread);
        }
        Ok(Self { axes, keys })
    }

    fn dims(&self) -> [usize; 3] {
        self.axes.each_ref().map(|a| a.0.len())
    }

    fn cell_of(&self, v: Coord) -> [usize; 3] {
        [0, 1, 2].map(|k| self.axes[k].find(i64::from(v[k] >> LOG[0])))
    }

    /// The key at a cell, if the cell is one root region holding one.
    fn key(&self, c: [usize; 3]) -> Option<Coord> {
        let r = [0, 1, 2].map(|k| self.axes[k].singleton(c[k]));
        let [Some(x), Some(y), Some(z)] = r else { return None };
        let key = [x, y, z].map(|r| i32::try_from(r << LOG[0]).ok());
        let [Some(x), Some(y), Some(z)] = key else { return None };
        self.keys.contains(&[x, y, z]).then_some([x, y, z])
    }

    fn unbounded(&self, c: [usize; 3]) -> bool {
        (0..3).any(|k| self.axes[k].unbounded(c[k]))
    }
}

/// Disjoint sets of units, each unit meeting the condition or not.
#[derive(Debug, Clone, Default)]
struct Units {
    parent: Vec<u32>,
    meets: Vec<bool>,
    /// Set when a unit could not be numbered; the labelling then fails.
    overflow: bool,
}

impl Units {
    fn add(&mut self, meets: bool) -> u32 {
        let Ok(id) = u32::try_from(self.parent.len()) else {
            self.overflow = true;
            return 0;
        };
        self.parent.push(id);
        self.meets.push(meets);
        id
    }

    fn len(&self) -> u32 {
        u32::try_from(self.parent.len()).unwrap_or(u32::MAX)
    }

    fn find(&mut self, mut x: u32) -> u32 {
        loop {
            let p = self.parent[x as usize];
            if p == x {
                return x;
            }
            let grand = self.parent[p as usize];
            self.parent[x as usize] = grand;
            x = grand;
        }
    }

    /// Joins two units that both meet the condition; the smaller root wins, so the result
    /// does not depend on the order of joins.
    fn join(&mut self, a: u32, b: u32) {
        if !(self.meets[a as usize] && self.meets[b as usize]) {
            return;
        }
        let (ra, rb) = (self.find(a), self.find(b));
        let (lo, hi) = (ra.min(rb), ra.max(rb));
        self.parent[hi as usize] = lo;
    }
}

/// What a region is at its level: one unit throughout, or split into children.
#[derive(Debug, Clone, Copy)]
enum Kind {
    Uniform(u32),
    Split,
}

/// Work on the explicit stack.
#[derive(Debug, Clone, Copy)]
enum Work {
    /// Join the touching children inside a region.
    Inside(Coord, usize),
    /// Join across two touching regions of one size, the second at offset `e`.
    Pair(Coord, Coord, usize, [i32; 3]),
    /// Join a region to a uniform unit touching it at offset `e`.
    Mixed(Coord, usize, u32, [i32; 3]),
}

/// The place of the region at `o`, of the size at `level`, in its node, x fastest.
fn place(o: Coord, level: usize) -> u32 {
    child_place(o, LOG[level], LOG[level - 1] - LOG[level])
}

/// The place, in the other of two regions `n` children a side apart along `e`, of the
/// neighbour by step `f` of the child at `c`.
fn across(c: [i32; 3], f: [i32; 3], e: [i32; 3], n: i32) -> [i32; 3] {
    [0, 1, 2].map(|k| c[k] + f[k] - e[k] * n)
}

/// The children per axis of a region at `level`.
fn per_axis(level: usize) -> i32 {
    1 << (LOG[level] - LOG[level + 1])
}

fn child(o: Coord, level: usize, c: [i32; 3]) -> Coord {
    [0, 1, 2].map(|k| o[k] + (c[k] << LOG[level + 1]))
}

fn children(level: usize) -> impl Iterator<Item = [i32; 3]> {
    let n = per_axis(level);
    (0..n).flat_map(move |z| (0..n).flat_map(move |y| (0..n).map(move |x| [x, y, z])))
}

/// Whether a child at `c` lies on the side of its region facing offset `e`.
fn on_side(c: [i32; 3], e: [i32; 3], n: i32) -> bool {
    (0..3).all(|k| match e[k] {
        1 => c[k] == n - 1,
        -1 => c[k] == 0,
        _ => true,
    })
}

struct Walk<'a> {
    t: &'a Topology,
    units: Units,
    voxel_base: u32,
    tile_base: u32,
    empty: BTreeMap<(usize, Coord), u32>,
    background: bool,
    offsets: Vec<[i32; 3]>,
    stack: Vec<Work>,
}

impl Walk<'_> {
    /// The unit of an empty place at `level` (1: in an upper node, 2: in a lower node).
    /// Each node's places get consecutive units when first needed, all excluded; a place
    /// found empty takes the background's condition.
    fn empty_unit(&mut self, level: usize, o: Coord) -> u32 {
        let node = align(o, LOG[level - 1]);
        let base = match self.empty.get(&(level, node)) {
            Some(b) => *b,
            None => {
                let n = per_axis(level - 1);
                let b = self.units.len();
                for _ in 0..n * n * n {
                    self.units.add(false);
                }
                self.empty.insert((level, node), b);
                b
            }
        };
        let u = base + place(o, level);
        if let Some(m) = self.units.meets.get_mut(u as usize) {
            *m = self.background;
        }
        u
    }

    /// The unit of the voxel at child `c` of the leaf at `r`, whose first voxel's unit is
    /// `base`: a leaf's voxels are found from its number once, not each from the root.
    fn voxel(base: u32, r: Coord, c: [i32; 3]) -> u32 {
        base + leaf_place(child(r, 2, c))
    }

    /// The unit of the first voxel of the leaf at `r`, which classifying it at level 2
    /// found split.
    fn leaf_base(&self, r: Coord) -> u32 {
        match self.t.locate(r) {
            Location::Leaf(n) => self.voxel_base + n.get() * 512,
            Location::Tile(..) | Location::Background => 0,
        }
    }

    fn classify(&mut self, r: Coord, level: usize) -> Kind {
        // A leaf's voxels are found by the leaf's own paths (`run`, `pair`, `kinds`), so no
        // region below level 2 is classified.
        match (level, self.t.locate(r)) {
            (2, Location::Leaf(_)) => Kind::Split,
            (_, Location::Tile(n, l)) if l.edge_log2() >= LOG[level] => {
                Kind::Uniform(self.tile_base + n.get())
            }
            (0, _) if self.t.has_upper_node(r) => Kind::Split,
            (1, _) if self.t.has_lower_node(r) => Kind::Split,
            _ => Kind::Uniform(self.empty_unit(level, r)),
        }
    }

    fn run(&mut self) {
        while let Some(w) = self.stack.pop() {
            match w {
                Work::Inside(r, l) => self.inside(r, l),
                Work::Pair(a, b, l, e) => self.pair(a, b, l, e),
                Work::Mixed(r, l, u, e) => match self.classify(r, l) {
                    Kind::Uniform(x) => self.units.join(x, u),
                    Kind::Split if l == 2 => {
                        let base = self.leaf_base(r);
                        for c in children(l).filter(|c| on_side(*c, e, per_axis(l))) {
                            self.units.join(Self::voxel(base, r, c), u);
                        }
                    }
                    Kind::Split => {
                        let n = per_axis(l);
                        for c in children(l).filter(|c| on_side(*c, e, n)) {
                            self.stack.push(Work::Mixed(child(r, l, c), l + 1, u, e));
                        }
                    }
                },
            }
        }
    }

    /// Every child's kind, x fastest. An upper node's are read from its masks, looking up
    /// only its tiles, since most of its places are usually empty.
    fn kinds(&mut self, r: Coord, l: usize) -> Vec<Kind> {
        if l == 2 {
            let base = self.leaf_base(r);
            return children(l).map(|c| Kind::Uniform(Self::voxel(base, r, c))).collect();
        }
        if l > 0 {
            return children(l).map(|c| self.classify(child(r, l, c), l + 1)).collect();
        }
        let places = self.t.upper_places(r);
        children(l)
            .zip(places)
            .map(|(c, p)| match p {
                Place::Child => Kind::Split,
                Place::Tile => self.classify(child(r, l, c), l + 1),
                Place::Empty => Kind::Uniform(self.empty_unit(1, child(r, l, c))),
            })
            .collect()
    }

    /// Joins the touching children of a split region, classifying each child once: most
    /// pairs are two uniform units, joined at once, and only pairs with a split child go on
    /// the stack.
    fn inside(&mut self, r: Coord, l: usize) {
        if matches!(self.classify(r, l), Kind::Uniform(_)) {
            return;
        }
        let n = per_axis(l);
        let kinds = self.kinds(r, l);
        let at = |c: [i32; 3]| usize::try_from(c[0] + n * (c[1] + n * c[2])).unwrap_or(0);
        let forward: Vec<[i32; 3]> = self
            .offsets
            .iter()
            .copied()
            .filter(|e| e.iter().find(|x| **x != 0).is_some_and(|x| *x > 0))
            .collect();
        for c in children(l) {
            let kc = kinds[at(c)];
            if matches!(kc, Kind::Split) {
                self.stack.push(Work::Inside(child(r, l, c), l + 1));
            }
            for e in &forward {
                let d = [c[0] + e[0], c[1] + e[1], c[2] + e[2]];
                if d.iter().all(|x| (0..n).contains(x)) {
                    let (a, b) = (child(r, l, c), child(r, l, d));
                    match (kc, kinds[at(d)]) {
                        (Kind::Uniform(x), Kind::Uniform(y)) => self.units.join(x, y),
                        (Kind::Split, Kind::Uniform(y)) => {
                            self.stack.push(Work::Mixed(a, l + 1, y, *e))
                        }
                        (Kind::Uniform(x), Kind::Split) => {
                            self.stack.push(Work::Mixed(b, l + 1, x, e.map(|v| -v)))
                        }
                        (Kind::Split, Kind::Split) => self.stack.push(Work::Pair(a, b, l + 1, *e)),
                    }
                }
            }
        }
    }

    fn pair(&mut self, a: Coord, b: Coord, l: usize, e: [i32; 3]) {
        match (self.classify(a, l), self.classify(b, l)) {
            (Kind::Uniform(x), Kind::Uniform(y)) => self.units.join(x, y),
            (Kind::Split, Kind::Uniform(y)) => self.stack.push(Work::Mixed(a, l, y, e)),
            (Kind::Uniform(x), Kind::Split) => self.stack.push(Work::Mixed(b, l, x, e.map(|c| -c))),
            (Kind::Split, Kind::Split) if l == 2 => {
                let (ba, bb, n) = (self.leaf_base(a), self.leaf_base(b), per_axis(l));
                for c in children(l).filter(|c| on_side(*c, e, n)) {
                    for f in &self.offsets {
                        let t = across(c, *f, e, n);
                        if t.iter().all(|x| (0..n).contains(x)) {
                            self.units.join(Self::voxel(ba, a, c), Self::voxel(bb, b, t));
                        }
                    }
                }
            }
            (Kind::Split, Kind::Split) => {
                let n = per_axis(l);
                for c in children(l).filter(|c| on_side(*c, e, n)) {
                    for f in &self.offsets {
                        let t = across(c, *f, e, n);
                        if t.iter().all(|x| (0..n).contains(x)) {
                            self.stack.push(Work::Pair(child(a, l, c), child(b, l, t), l + 1, *f));
                        }
                    }
                }
            }
        }
    }
}

/// The components of a topology and field under a condition.
#[derive(Debug, Clone)]
pub struct Labels {
    stamp: u64,
    /// Each unit's label, `EXCLUDED` or its component's number: 4 bytes a voxel.
    labels: Vec<u32>,
    voxel_base: u32,
    tile_base: u32,
    empty: BTreeMap<(usize, Coord), u32>,
    root: RootGrid,
    root_units: BTreeMap<[usize; 3], u32>,
    outside: u32,
    count: u32,
}

impl Walk<'_> {
    /// What a root cell is: the outside, an empty region, or a key's region.
    fn root_kind(
        &mut self,
        root: &RootGrid,
        c: [usize; 3],
        units: &mut BTreeMap<[usize; 3], u32>,
        outside: u32,
    ) -> (Kind, Coord) {
        if root.unbounded(c) {
            return (Kind::Uniform(outside), [0; 3]);
        }
        if let Some(key) = root.key(c) {
            return (self.classify(key, 0), key);
        }
        let u = *units.entry(c).or_insert_with(|| self.units.add(self.background));
        (Kind::Uniform(u), [0; 3])
    }

    /// Joins across every pair of touching root cells, and queues every upper node.
    fn root(&mut self, root: &RootGrid, outside: u32) -> BTreeMap<[usize; 3], u32> {
        let mut units = BTreeMap::new();
        let [nx, ny, nz] = root.dims();
        let cells =
            (0..nz).flat_map(|z| (0..ny).flat_map(move |y| (0..nx).map(move |x| [x, y, z])));
        let offsets = self.offsets.clone();
        for c in cells {
            let (kc, a) = self.root_kind(root, c, &mut units, outside);
            if matches!(kc, Kind::Split) {
                self.stack.push(Work::Inside(a, 0));
            }
            for e in offsets.iter().filter(|e| e.iter().find(|x| **x != 0).is_some_and(|x| *x > 0))
            {
                let d = [0, 1, 2].map(|k| c[k].checked_add_signed(e[k] as isize));
                let [Some(dx), Some(dy), Some(dz)] = d else { continue };
                if dx >= nx || dy >= ny || dz >= nz {
                    continue;
                }
                let (kd, b) = self.root_kind(root, [dx, dy, dz], &mut units, outside);
                match (kc, kd) {
                    (Kind::Uniform(x), Kind::Uniform(y)) => self.units.join(x, y),
                    (Kind::Split, Kind::Uniform(y)) => self.stack.push(Work::Mixed(a, 0, y, *e)),
                    (Kind::Uniform(x), Kind::Split) => {
                        self.stack.push(Work::Mixed(b, 0, x, e.map(|v| -v)))
                    }
                    (Kind::Split, Kind::Split) => self.stack.push(Work::Pair(a, b, 0, *e)),
                }
            }
        }
        units
    }
}

/// Refuses a topology whose units, the outside, each leaf voxel and each tile, a `u32` does
/// not number, before any is made: the walk finds a voxel's unit as a leaf's first plus
/// 512 times its number.
fn fits_units(leaves: usize, tiles: usize) -> Result<(), AnalysisError> {
    let wide = |n: usize| u64::try_from(n).unwrap_or(u64::MAX);
    let units = wide(leaves).saturating_mul(512).saturating_add(wide(tiles)).saturating_add(1);
    if units > u64::from(u32::MAX) { Err(AnalysisError::TooLarge) } else { Ok(()) }
}

/// Labels the voxels of `f` whose value meets `meets`, with the given connectivity.
pub fn label<T: Value>(
    t: &Topology,
    f: &Field<T>,
    meets: impl Fn(T) -> bool,
    conn: Connectivity,
) -> Result<Labels, AnalysisError> {
    f.belongs_to(t)?;
    fits_units(t.leaf_count(), t.tile_count())?;
    let root = RootGrid::new(t)?;
    let background = meets(f.background());
    let mut units = Units::default();
    let outside = units.add(background);
    let voxel_base = units.len();
    for n in t.leaf_numbers() {
        for v in f.leaf(t, n)? {
            units.add(meets(*v));
        }
    }
    let tile_base = units.len();
    for n in t.tile_numbers() {
        units.add(meets(f.tile(t, n)?));
    }
    let mut walk = Walk {
        t,
        units,
        voxel_base,
        tile_base,
        empty: BTreeMap::new(),
        background,
        offsets: conn.offsets(),
        stack: Vec::new(),
    };
    let root_units = walk.root(&root, outside);
    walk.run();
    if walk.units.overflow {
        return Err(AnalysisError::TooLarge);
    }
    let (labels, count) = number(walk.units)?;
    Ok(Labels {
        stamp: t.stamp(),
        labels,
        voxel_base,
        tile_base,
        empty: walk.empty,
        root,
        root_units,
        outside,
        count,
    })
}

/// A unit's stored label when it does not meet the condition; a component's is its number.
const EXCLUDED: u32 = u32::MAX;

/// Numbers the components in the order of their first unit, writing each unit's label over
/// its parent: a unit's parent is never a later unit, since a join links the larger root
/// under the smaller and `find` only shortens paths, so the parent is numbered first and
/// its label is the unit's. A component's root is its smallest unit, so it is met first.
fn number(units: Units) -> Result<(Vec<u32>, u32), AnalysisError> {
    let Units { parent: mut labels, meets, .. } = units;
    let mut count = 0;
    for (id, meets) in meets.into_iter().enumerate() {
        let parent = labels[id] as usize;
        labels[id] = if !meets {
            EXCLUDED
        } else if parent == id {
            if count == EXCLUDED {
                return Err(AnalysisError::TooLarge);
            }
            count += 1;
            count - 1
        } else {
            labels[parent]
        };
    }
    Ok((labels, count))
}

/// A stored label as a `Label`.
fn decode(stored: u32) -> Label {
    if stored == EXCLUDED { Label::Excluded } else { Label::Component(stored) }
}

impl Labels {
    /// The number of components.
    pub fn count(&self) -> u32 {
        self.count
    }

    fn unit_at(&self, t: &Topology, v: Coord) -> Option<u32> {
        match t.locate(v) {
            Location::Leaf(n) => Some(self.voxel_base + n.get() * 512 + leaf_place(v)),
            Location::Tile(n, _) => Some(self.tile_base + n.get()),
            Location::Background if t.has_lower_node(v) => self
                .empty
                .get(&(2, TileLevel::Upper.origin(v)))
                .map(|b| b + place(TileLevel::Lower.origin(v), 2)),
            Location::Background if t.has_upper_node(v) => self
                .empty
                .get(&(1, TileLevel::Root.origin(v)))
                .map(|b| b + place(TileLevel::Upper.origin(v), 1)),
            Location::Background => {
                let c = self.root.cell_of(v);
                if self.root.unbounded(c) {
                    Some(self.outside)
                } else {
                    self.root_units.get(&c).copied()
                }
            }
        }
    }

    /// The label of a voxel; `t` must be the topology labelled.
    pub fn label_at(&self, t: &Topology, v: Coord) -> Result<Label, AnalysisError> {
        if t.stamp() != self.stamp {
            return Err(AnalysisError::Field(FieldError::StaleField));
        }
        let u = self.unit_at(t, v).ok_or(AnalysisError::Unlabelled(v))?;
        self.labels.get(u as usize).map(|x| decode(*x)).ok_or(AnalysisError::Unlabelled(v))
    }

    /// Whether a component reaches the unbounded background outside the tree.
    pub fn reaches_outside(&self, label: Label) -> bool {
        label != Label::Excluded
            && self.labels.get(self.outside as usize).map(|x| decode(*x)) == Some(label)
    }
}

/// The component of a seed voxel (`Label::Excluded` if the seed does not meet the
/// condition).
#[derive(Debug, Clone)]
pub struct Flood {
    labels: Labels,
    label: Label,
}

impl Flood {
    /// The seed's label.
    pub fn label(&self) -> Label {
        self.label
    }

    /// Whether the voxel is in the seed's component.
    pub fn contains(&self, t: &Topology, v: Coord) -> Result<bool, AnalysisError> {
        Ok(self.label != Label::Excluded && self.labels.label_at(t, v)? == self.label)
    }

    /// Whether the seed's component reaches the outside.
    pub fn reaches_outside(&self) -> bool {
        self.labels.reaches_outside(self.label)
    }
}

/// Flood fill from `seed` over the voxels whose value meets `meets`: the seed's component
/// of the labelling (ADR-0013).
pub fn flood_fill<T: Value>(
    t: &Topology,
    f: &Field<T>,
    meets: impl Fn(T) -> bool,
    conn: Connectivity,
    seed: Coord,
) -> Result<Flood, AnalysisError> {
    let labels = label(t, f, meets, conn)?;
    let label = labels.label_at(t, seed)?;
    Ok(Flood { labels, label })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn more_units_than_a_u32_numbers_are_refused_before_any_is_made() {
        // The outside, 512 units a leaf and one a tile: 2^23 - 1 leaves leave 511 units.
        let leaves = (1 << 23) - 1;
        assert!(fits_units(leaves, 510).is_ok());
        assert!(matches!(fits_units(leaves, 511), Err(AnalysisError::TooLarge)));
        assert!(matches!(fits_units(1 << 23, 0), Err(AnalysisError::TooLarge)));
    }
    use crate::topology::TopologyEditor;

    fn topology(edit: impl FnOnce(&mut TopologyEditor<'_>)) -> Topology {
        let empty = Topology::new();
        let mut e = empty.edit();
        edit(&mut e);
        e.commit().unwrap().0
    }

    fn positive(v: f32) -> bool {
        v > 0.0
    }

    #[test]
    fn background_enclosed_by_leaves_is_apart_from_the_outside() {
        // A shell of leaves one region thick around the region at the origin.
        let t = topology(|e| {
            for i in 0..27 {
                let d = [i % 3 - 1, i / 3 % 3 - 1, i / 9 - 1];
                if d != [0, 0, 0] {
                    e.activate(d.map(|c| c * 8)).unwrap();
                }
            }
        });
        let mut f = Field::new(&t, 1.0_f32);
        for n in t.leaf_numbers().collect::<Vec<_>>() {
            f.leaf_mut(&t, n).unwrap().fill(-1.0);
        }
        let l = label(&t, &f, positive, Connectivity::TwentySix).unwrap();
        assert_eq!(l.count(), 2);
        let inside = l.label_at(&t, [3, 3, 3]).unwrap();
        let outside = l.label_at(&t, [100, 0, 0]).unwrap();
        assert_ne!(inside, outside);
        assert!(!l.reaches_outside(inside) && l.reaches_outside(outside));
        assert_eq!(l.label_at(&t, [-8, 0, 0]).unwrap(), Label::Excluded);
    }

    /// Labelling a small grid ends: a mutant that makes it loop for ever fails here within a
    /// minute, in the first test binary, before the tests that would only wait for it
    /// (TESTING.md T11).
    #[test]
    fn labelling_a_small_grid_ends_in_time() {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let t = topology(|e| {
                e.activate([0, 0, 0]).unwrap();
                e.add_tile(TileLevel::Lower, [8, 0, 0]).unwrap();
            });
            let f = Field::new(&t, 1.0_f32);
            tx.send(label(&t, &f, positive, Connectivity::Six).map(|l| l.count())).unwrap_or(());
        });
        assert_eq!(rx.recv_timeout(std::time::Duration::from_secs(60)), Ok(Ok(1)));
    }

    #[test]
    fn six_and_twenty_six_neighbours_are_one_step_away() {
        for (conn, n) in [(Connectivity::Six, 6), (Connectivity::TwentySix, 26)] {
            let o = conn.offsets();
            assert_eq!(o.len(), n, "{conn:?}");
            assert!(o.iter().all(|d| *d != [0, 0, 0] && d.iter().all(|c| c.abs() <= 1)));
        }
    }

    /// Voxels in an upper node's empty places, where no lower node is, are labelled through
    /// the node's own empty units, not the root grid: here they are the outside's, and a
    /// flood from one reaches it. Eight leaves that do not meet the condition number the
    /// first 4096 units after the outside, so a unit miscounted from an empty place's lands
    /// among them, excluded.
    #[test]
    fn an_upper_nodes_empty_places_are_the_outsides() {
        let t = topology(|e| {
            for x in 0..8 {
                e.activate([x * 8, 0, 0]).unwrap();
            }
        });
        let mut f = Field::new(&t, 1.0_f32);
        for n in t.leaf_numbers().collect::<Vec<_>>() {
            f.leaf_mut(&t, n).unwrap().fill(-1.0);
        }
        let l = label(&t, &f, positive, Connectivity::Six).unwrap();
        let far = l.label_at(&t, [1_000_000, 0, 0]).unwrap();
        assert_ne!(far, Label::Excluded);
        for v in [[130, 0, 0], [500, 300, 200], [4095, 4095, 4095]] {
            assert_eq!(l.label_at(&t, v), Ok(far), "{v:?}");
        }
        let fill = flood_fill(&t, &f, positive, Connectivity::Six, [500, 300, 200]).unwrap();
        assert!(fill.reaches_outside());
        assert!(fill.contains(&t, [64, 0, 0]).unwrap() && !fill.contains(&t, [63, 0, 0]).unwrap());
    }

    #[test]
    fn a_corner_joins_under_26_and_not_under_6() {
        let t = topology(|e| {
            e.activate([0, 0, 0]).unwrap();
        });
        let mut f = Field::new(&t, -1.0_f32);
        let n = t.leaf_numbers().next().unwrap();
        let leaf = f.leaf_mut(&t, n).unwrap();
        leaf.fill(-1.0);
        leaf[0] = 1.0;
        leaf[1 + 8 + 64] = 1.0;
        assert_eq!(label(&t, &f, positive, Connectivity::Six).unwrap().count(), 2);
        assert_eq!(label(&t, &f, positive, Connectivity::TwentySix).unwrap().count(), 1);
    }

    #[test]
    fn root_tiles_can_enclose_a_root_region() {
        let t = topology(|e| {
            for i in 0..27 {
                let d = [i % 3 - 1, i / 3 % 3 - 1, i / 9 - 1];
                if d != [0, 0, 0] {
                    e.add_tile(TileLevel::Root, d.map(|c| c * 4096)).unwrap();
                }
            }
        });
        let f = Field::new(&t, 1.0_f32);
        let mut f = f;
        for n in t.tile_numbers().collect::<Vec<_>>() {
            f.set_tile(&t, n, -1.0).unwrap();
        }
        let l = label(&t, &f, positive, Connectivity::Six).unwrap();
        let centre = l.label_at(&t, [100, 100, 100]).unwrap();
        let far = l.label_at(&t, [1_000_000, 0, 0]).unwrap();
        assert_eq!(l.count(), 2);
        assert!(!l.reaches_outside(centre) && l.reaches_outside(far));
    }

    #[test]
    fn tiles_join_what_touches_them() {
        let t = topology(|e| {
            e.add_tile(TileLevel::Upper, [0, 0, 0]).unwrap();
            e.activate([128, 0, 0]).unwrap();
            e.add_tile(TileLevel::Lower, [200, 0, 0]).unwrap();
        });
        let mut f = Field::new(&t, -1.0_f32);
        for n in t.tile_numbers().collect::<Vec<_>>() {
            f.set_tile(&t, n, 1.0).unwrap();
        }
        let n = t.leaf_numbers().next().unwrap();
        f.leaf_mut(&t, n).unwrap()[8 * 3 + 64 * 5] = 1.0;
        let fill = flood_fill(&t, &f, positive, Connectivity::Six, [5, 3, 5]).unwrap();
        assert!(fill.contains(&t, [128, 3, 5]).unwrap(), "the leaf voxel on the tile's face");
        assert!(!fill.contains(&t, [129, 3, 5]).unwrap());
        assert!(!fill.contains(&t, [200, 0, 0]).unwrap(), "the far tile is apart");
        assert!(!fill.reaches_outside());
        // A seed that does not meet the condition holds nothing, not even itself or the other
        // voxels that do not meet it (W5 of P5's review).
        let none = flood_fill(&t, &f, positive, Connectivity::Six, [129, 3, 5]).unwrap();
        assert_eq!(none.label(), Label::Excluded);
        for v in [[129, 3, 5], [130, 0, 0], [5000, 0, 0]] {
            assert!(!none.contains(&t, v).unwrap(), "{v:?}");
        }
        let (t2, _) = t.edit().commit().unwrap();
        assert_eq!(
            fill.contains(&t2, [0, 0, 0]),
            Err(AnalysisError::Field(FieldError::StaleField))
        );
    }
}
