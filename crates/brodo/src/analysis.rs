//! Topology analysis (ARCHITECTURE.md §4.5, ADR-0013): connected components of the voxels
//! whose value meets a condition, tiles and the background included, flood fill, and the
//! Euler characteristic (`euler`, ADR-0024).
//!
//! The units joined are leaf voxels, tiles, the empty places of lower nodes (8³) and upper
//! nodes (128³), the empty regions at the root, as runs between the root's keys within
//! their bounding box, and the unbounded outside. Two units are joined when both meet
//! the condition and they touch: across a face (6-connectivity), or across a face, an edge
//! or a corner (26-connectivity). Touching regions are found by descending the levels with
//! an explicit stack: a pair of regions of one size, or a region against a uniform unit.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::{AnalysisError, FieldError};
use crate::field::{Field, Value, leaf_place};
use crate::topology::{Location, Topology};
use crate::tree::{Coord, Place, TileLevel, align, child_place};
use crate::tree::{LEAF_LOG2, LOWER_SPAN_LOG2, UPPER_SPAN_LOG2};

mod euler;
pub use euler::euler;

/// Which neighbours touch; another may be added, such as 18 neighbours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
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

    /// How many coordinates two touching regions may differ in.
    fn reach(self) -> i64 {
        match self {
            Connectivity::Six => 1,
            Connectivity::TwentySix => 3,
        }
    }
}

/// A voxel's label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Label {
    /// The component, numbered in the order of its first unit: the outside, then each
    /// leaf's voxels by leaf number, x fastest, then the tiles by tile number, then the
    /// empty regions. A component holding the outside is component 0.
    Component(u32),
    /// The voxel's value does not meet the condition.
    Excluded,
}

/// log2 of a region's edge at each level: a root region, a lower node's, a leaf's, a voxel.
const LOG: [u32; 4] = [UPPER_SPAN_LOG2, LOWER_SPAN_LOG2, LEAF_LOG2, 0];

/// A root region's coordinates: a voxel's shifted by a root region's log2 edge.
type Region = [i64; 3];

fn region_of(v: Coord) -> Region {
    v.map(|c| i64::from(c >> LOG[0]))
}

/// The offsets whose first non-zero component is positive: each touching pair once.
fn forward(offsets: &[[i32; 3]]) -> impl Iterator<Item = &[i32; 3]> {
    offsets.iter().filter(|e| e.iter().find(|x| **x != 0).is_some_and(|x| *x > 0))
}

/// Root regions along x, none holding a key, and their unit.
#[derive(Debug, Clone, Copy)]
struct Run {
    x: (i64, i64),
    unit: u32,
}

/// Root regions along y at each z of a slab: one run across the box, or, at a key's y in a
/// key's z, the runs between its keys.
#[derive(Debug, Clone)]
struct Row {
    y: (i64, i64),
    runs: Vec<Run>,
}

/// Root regions along z, and their rows.
#[derive(Debug, Clone)]
struct Slab {
    z: (i64, i64),
    rows: Vec<Row>,
}

/// A root region to the labelling walk: the outside or an empty region, by its unit, or a
/// key's region, classified, and its key.
#[derive(Debug, Clone, Copy)]
enum AtRoot {
    Free(u32),
    Key(Kind, Coord),
}

/// What a root region is.
#[derive(Debug, Clone, Copy)]
enum RootRegion {
    Outside,
    Key(Coord),
    Empty(u32),
}

/// The root's empty regions (ADR-0013): within the keys' bounding box, grown by one region
/// along each axis, as runs of keyless z, of keyless y in a keyed z, and of x between the
/// keys of a keyed row, each one unit; the regions beyond the box are the outside's.
#[derive(Debug, Clone)]
struct RootRegions {
    lo: Region,
    hi: Region,
    keys: BTreeMap<Region, Coord>,
    slabs: Vec<Slab>,
}

impl RootRegions {
    /// Makes a unit for each run, in z, y, x order, and joins the runs that touch, and
    /// those on the box's rim to `outside`.
    fn new(t: &Topology, units: &mut Units, background: bool, outside: u32, reach: i64) -> Self {
        let keys: BTreeMap<Region, Coord> = t.root_keys().map(|k| (region_of(k), k)).collect();
        let mut by_z: BTreeMap<i64, BTreeMap<i64, Vec<i64>>> = BTreeMap::new();
        for &[x, y, z] in keys.keys() {
            by_z.entry(z).or_default().entry(y).or_default().push(x);
        }
        let bound = |k: usize, f: fn(i64, i64) -> i64| keys.keys().map(|c| c[k]).reduce(f);
        let (Some(x0), Some(y0), Some(z0)) =
            (bound(0, i64::min), bound(1, i64::min), bound(2, i64::min))
        else {
            return Self { lo: [0; 3], hi: [0; 3], keys, slabs: vec![] };
        };
        let (x1, y1, z1) = (bound(0, i64::max), bound(1, i64::max), bound(2, i64::max));
        let lo = [x0 - 1, y0 - 1, z0 - 1];
        let hi = [x1.unwrap_or(x0) + 1, y1.unwrap_or(y0) + 1, z1.unwrap_or(z0) + 1];
        let whole = |units: &mut Units, y| Row {
            y,
            runs: vec![Run { x: (lo[0], hi[0]), unit: units.add(background) }],
        };
        let mut slabs = vec![];
        let mut z = lo[2];
        for (&kz, rows) in &by_z {
            if z < kz {
                slabs.push(Slab { z: (z, kz - 1), rows: vec![whole(units, (lo[1], hi[1]))] });
            }
            let mut out = vec![];
            let mut y = lo[1];
            for (&ky, xs) in rows {
                if y < ky {
                    out.push(whole(units, (y, ky - 1)));
                }
                let mut runs = vec![];
                let mut x = lo[0];
                for &kx in xs {
                    if x < kx {
                        runs.push(Run { x: (x, kx - 1), unit: units.add(background) });
                    }
                    x = kx + 1;
                }
                runs.push(Run { x: (x, hi[0]), unit: units.add(background) });
                out.push(Row { y: (ky, ky), runs });
                y = ky + 1;
            }
            out.push(whole(units, (y, hi[1])));
            slabs.push(Slab { z: (kz, kz), rows: out });
            z = kz + 1;
        }
        slabs.push(Slab { z: (z, hi[2]), rows: vec![whole(units, (lo[1], hi[1]))] });
        let root = Self { lo, hi, keys, slabs };
        root.join(units, outside, reach);
        root
    }

    /// Joins every two touching runs, `reach` being how many coordinates two touching
    /// regions may differ in: a run on the rim and the outside, beyond it; runs of the rows
    /// of a slab, along y; and of the next slab's rows, along z.
    fn join(&self, units: &mut Units, outside: u32, reach: i64) {
        let (lo, hi) = (self.lo, self.hi);
        for (i, s) in self.slabs.iter().enumerate() {
            for row in &s.rows {
                for r in &row.runs {
                    let rim = [(s.z, 2), (row.y, 1), (r.x, 0)];
                    if rim.iter().any(|((a, b), k)| *a == lo[*k] || *b == hi[*k]) {
                        units.join(outside, r.unit);
                    }
                }
            }
            for w in s.rows.windows(2) {
                touch(units, &w[0], &w[1], 1, reach);
            }
            let Some(next) = self.slabs.get(i + 1) else { continue };
            for a in &s.rows {
                let first = next.rows.partition_point(|b| b.y.1 < a.y.0 - 1);
                for b in next.rows[first..].iter().take_while(|b| b.y.0 <= a.y.1 + 1) {
                    let dy = i64::from(b.y.0 > a.y.1 || a.y.0 > b.y.1);
                    touch(units, a, b, 1 + dy, reach);
                }
            }
        }
    }

    /// What a root region is.
    fn locate(&self, r: Region) -> RootRegion {
        if self.slabs.is_empty() || (0..3).any(|k| r[k] < self.lo[k] || r[k] > self.hi[k]) {
            return RootRegion::Outside;
        }
        let s = &self.slabs[self.slabs.partition_point(|s| s.z.1 < r[2])];
        let row = &s.rows[s.rows.partition_point(|w| w.y.1 < r[1])];
        match row.runs.get(row.runs.partition_point(|q| q.x.1 < r[0])) {
            Some(q) if q.x.0 <= r[0] => RootRegion::Empty(q.unit),
            _ => self.keys.get(&r).map_or(RootRegion::Outside, |k| RootRegion::Key(*k)),
        }
    }
}

/// Joins the runs of two rows whose regions differ in `apart` coordinates besides x, where
/// their x overlaps or, if `reach` allows one more, touches.
fn touch(units: &mut Units, a: &Row, b: &Row, apart: i64, reach: i64) {
    if apart > reach {
        return;
    }
    let tol = (reach - apart).min(1);
    for r in &a.runs {
        let first = b.runs.partition_point(|q| q.x.1 < r.x.0 - tol);
        for q in b.runs[first..].iter().take_while(|q| q.x.0 <= r.x.1 + tol) {
            units.join(r.unit, q.unit);
        }
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
        // Units beyond a `u32` set the overflow the labelling is then refused for; the walk
        // goes on with unit 0, its labels never read.
        let Some(u) = base.checked_add(place(o, level)) else {
            self.units.overflow = true;
            return 0;
        };
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
    root: RootRegions,
    outside: u32,
    count: u32,
}

impl Walk<'_> {
    /// What a root region is to the walk: the outside's unit or an empty region's, or a
    /// key's region classified.
    fn root_kind(&mut self, root: &RootRegions, r: Region, outside: u32) -> AtRoot {
        match root.locate(r) {
            RootRegion::Outside => AtRoot::Free(outside),
            RootRegion::Empty(u) => AtRoot::Free(u),
            RootRegion::Key(key) => AtRoot::Key(self.classify(key, 0), key),
        }
    }

    /// Joins across every pair of touching root regions one of which holds a key, and
    /// queues every upper node: the regions in z, y, x order, each against its neighbours
    /// in the order of `forward`, so that the work is queued in one order. Empty regions
    /// and the outside were joined as `RootRegions` was made.
    fn root(&mut self, root: &RootRegions, outside: u32) {
        let offsets = self.offsets.clone();
        let mut regions = BTreeSet::new();
        for k in root.keys.keys() {
            regions.insert([k[2], k[1], k[0]]);
            for e in forward(&offsets) {
                regions.insert([0, 1, 2].map(|i| k[2 - i] - i64::from(e[2 - i])));
            }
        }
        for [z, y, x] in regions {
            let c = [x, y, z];
            let at = self.root_kind(root, c, outside);
            if let AtRoot::Key(Kind::Split, a) = at {
                self.stack.push(Work::Inside(a, 0));
            }
            for e in forward(&offsets) {
                let d = [0, 1, 2].map(|k| c[k] + i64::from(e[k]));
                match (at, self.root_kind(root, d, outside)) {
                    (AtRoot::Free(_), AtRoot::Free(_)) => {}
                    (
                        AtRoot::Free(x) | AtRoot::Key(Kind::Uniform(x), _),
                        AtRoot::Free(y) | AtRoot::Key(Kind::Uniform(y), _),
                    ) => self.units.join(x, y),
                    (
                        AtRoot::Key(Kind::Split, a),
                        AtRoot::Free(y) | AtRoot::Key(Kind::Uniform(y), _),
                    ) => self.stack.push(Work::Mixed(a, 0, y, *e)),
                    (
                        AtRoot::Free(x) | AtRoot::Key(Kind::Uniform(x), _),
                        AtRoot::Key(Kind::Split, b),
                    ) => self.stack.push(Work::Mixed(b, 0, x, e.map(|v| -v))),
                    (AtRoot::Key(Kind::Split, a), AtRoot::Key(Kind::Split, b)) => {
                        self.stack.push(Work::Pair(a, b, 0, *e))
                    }
                }
            }
        }
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
///
/// Refused as `TooLarge`, before any label is made, when the outside, 512 units for each
/// leaf and one for each tile are more than a `u32` numbers, about 8.4 million leaves, or
/// when the empty regions found as it walks bring them past it. The labels hold four bytes
/// per leaf voxel.
pub fn label<T: Value>(
    t: &Topology,
    f: &Field<T>,
    meets: impl Fn(T) -> bool,
    conn: Connectivity,
) -> Result<Labels, AnalysisError> {
    f.belongs_to(t)?;
    fits_units(t.leaf_count(), t.tile_count())?;
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
    let root = RootRegions::new(t, &mut walk.units, background, outside, conn.reach());
    walk.root(&root, outside);
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
            Location::Background => match self.root.locate(region_of(v)) {
                RootRegion::Outside => Some(self.outside),
                RootRegion::Empty(u) => Some(u),
                RootRegion::Key(_) => None,
            },
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
        let label = self.labels.label_at(t, v)?;
        Ok(self.label != Label::Excluded && label == self.label)
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
        // In the upper node, in no lower node: each of its lower regions is a unit.
        for v in [[200, 200, 200], [4000, 300, 2000]] {
            assert_eq!(l.label_at(&t, v), Ok(outside), "{v:?}");
        }
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
    /// the node's own empty units, not the root's: here they are the outside's, and a
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

    /// Keys far apart along every axis, a leaf and root tiles, are labelled, each its own
    /// component, and the regions between and beyond them the outside's: a grid of the
    /// root's regions compressed around them would hold some 180³ cells, past the 2²² once
    /// refused (ADR-0013's amendment of 2026-11-11).
    #[test]
    fn keys_scattered_along_every_axis_are_labelled() {
        let at = |i: i32| [i * 4 * 4096, (44 - i) * 5 * 4096, (i * 7 % 45) * 6 * 4096];
        let t = topology(|e| {
            e.activate(at(0)).unwrap();
            for i in 1..45 {
                e.add_tile(TileLevel::Root, at(i)).unwrap();
            }
        });
        let mut f = Field::new(&t, -1.0_f32);
        for n in t.tile_numbers().collect::<Vec<_>>() {
            f.set_tile(&t, n, 1.0).unwrap();
        }
        let n = t.leaf_numbers().next().unwrap();
        f.leaf_mut(&t, n).unwrap()[0] = 1.0;
        for conn in [Connectivity::Six, Connectivity::TwentySix] {
            let l = label(&t, &f, positive, conn).unwrap();
            assert_eq!(l.count(), 45, "{conn:?}");
            assert_ne!(l.label_at(&t, at(0)).unwrap(), l.label_at(&t, at(1)).unwrap());
            let inv = label(&t, &f, |v| v < 0.0, conn).unwrap();
            let far = inv.label_at(&t, [-2_000_000_000, 0, 0]).unwrap();
            assert!(inv.reaches_outside(far));
            for v in [[4096 * 3 + 4000, 4096 * 7, 4096 * 11], at(0).map(|c| c + 4095)] {
                assert_eq!(inv.label_at(&t, v), Ok(far), "{v:?} {conn:?}");
            }
        }
    }

    /// Two root regions enclosed by root tiles that touch only along an edge, in each
    /// plane, or at a corner, are one component under 26-connectivity and two under 6,
    /// neither the outside's.
    #[test]
    fn root_regions_touching_at_an_edge_or_a_corner_join_under_26_only() {
        // Along an edge in each plane, each way, and at a corner.
        let pairs = [
            [[1, 1, 1], [2, 2, 1]],
            [[2, 1, 1], [1, 2, 1]],
            [[1, 1, 1], [1, 2, 2]],
            [[1, 2, 1], [1, 1, 2]],
            [[1, 1, 1], [2, 1, 2]],
            [[2, 1, 1], [1, 1, 2]],
            [[1, 1, 1], [2, 2, 2]],
            [[2, 2, 1], [1, 1, 2]],
        ];
        for holes in pairs {
            let t = topology(|e| {
                for i in 0..64 {
                    let c = [i % 4, i / 4 % 4, i / 16];
                    if !holes.contains(&c) {
                        e.add_tile(TileLevel::Root, c.map(|v| v * 4096)).unwrap();
                    }
                }
            });
            let mut f = Field::new(&t, 1.0_f32);
            for n in t.tile_numbers().collect::<Vec<_>>() {
                f.set_tile(&t, n, -1.0).unwrap();
            }
            let in_hole = |h: [i32; 3]| h.map(|v| v * 4096 + 9);
            for (conn, count) in [(Connectivity::Six, 3), (Connectivity::TwentySix, 2)] {
                let l = label(&t, &f, positive, conn).unwrap();
                assert_eq!(l.count(), count, "{conn:?} {holes:?}");
                let a = l.label_at(&t, in_hole(holes[0])).unwrap();
                let b = l.label_at(&t, in_hole(holes[1])).unwrap();
                assert_eq!(a == b, conn == Connectivity::TwentySix, "{holes:?}");
                assert!(!l.reaches_outside(a) && !l.reaches_outside(b));
            }
        }
    }

    /// Labelling a leaf in each of 16 root regions ends at once: the walk takes an upper node's empty places whole,
    /// and a mutant that split each of them into its 4096 lower places, the same labels
    /// found by 10⁸ more steps, fails here, in the first test binary (TESTING.md T11).
    #[test]
    fn labelling_a_few_leaves_ends_in_time() {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            // A leaf in each of 16 root regions, each holding an upper node of 32 767
            // empty places.
            let t = topology(|e| {
                for i in 0..16 {
                    e.activate([4096 * i, 0, 0]).unwrap();
                }
            });
            let mut f = Field::new(&t, -1.0_f32);
            for n in t.leaf_numbers() {
                f.leaf_mut(&t, n).unwrap().fill(1.0);
            }
            tx.send(label(&t, &f, positive, Connectivity::Six).map(|l| l.count())).unwrap_or(());
        });
        let got = rx.recv_timeout(std::time::Duration::from_secs(20));
        assert!(matches!(got, Ok(Ok(16))), "{got:?}");
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
        for flood in [&fill, &none] {
            assert_eq!(
                flood.contains(&t2, [0, 0, 0]),
                Err(AnalysisError::Field(FieldError::StaleField))
            );
        }
    }
}
