//! B-025: the root's empty regions for labelling, numbered two ways.
//!
//! The input is the root's keys as root-region coordinates (a key's origin shifted right by
//! a root region's log2 edge, as `analysis.rs` does). Both forms answer the same two
//! things the labelling walk asks of the root: what each root region is (the unbounded
//! outside, a key's region, or an empty region and its unit), and so which units the walk
//! joins at the root. Keys block; the walk's joins between a key's region and what touches
//! it are made from `locate` of each neighbour, the same in both.
//!
//! - `Dense` is `analysis.rs`'s `RootGrid` and `Walk::root`: compressed axes, every cell
//!   of their product visited, one unit per empty cell made in the order the walk makes
//!   them, joined across every touching pair, refused past `ROOT_CELLS` cells.
//! - `Sparse` holds the empty space between the keys' bounding box, grown by one region,
//!   as boxes: runs of keyless z as one slab, and in each keyed z, runs of keyless y as one
//!   row, and in each keyed row, the runs of x between its keys. One unit per box, boxes
//!   joined where they touch; a box on the bounding box's rim touches the outside.

use std::collections::{BTreeMap, BTreeSet};

pub type R = [i64; 3];

/// The largest compressed root grid, in cells, as `analysis.rs` has it.
pub const ROOT_CELLS: usize = 1 << 22;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conn {
    Six,
    TwentySix,
}

impl Conn {
    /// The offsets whose first non-zero component is positive, as `Walk::root` takes them.
    pub fn forward(self) -> Vec<[i64; 3]> {
        self.all().into_iter().filter(|e| e.iter().find(|x| **x != 0).is_some_and(|x| *x > 0)).collect()
    }

    pub fn all(self) -> Vec<[i64; 3]> {
        let mut out = vec![];
        for z in -1..=1_i64 {
            for y in -1..=1_i64 {
                for x in -1..=1_i64 {
                    let n = x.abs() + y.abs() + z.abs();
                    if n > 0 && (self == Conn::TwentySix || n == 1) {
                        out.push([x, y, z]);
                    }
                }
            }
        }
        out
    }

    /// How many coordinates two touching regions may differ in.
    fn reach(self) -> i64 {
        match self {
            Conn::Six => 1,
            Conn::TwentySix => 3,
        }
    }
}

/// What a root region is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Loc {
    Outside,
    Key,
    /// An empty region's unit.
    Empty(u32),
}

/// Disjoint sets, the smaller root winning, as `analysis.rs`'s `Units`.
#[derive(Debug, Clone, Default)]
pub struct Sets(pub Vec<u32>);

impl Sets {
    pub fn add(&mut self) -> u32 {
        let id = self.0.len() as u32;
        self.0.push(id);
        id
    }

    pub fn find(&mut self, mut x: u32) -> u32 {
        loop {
            let p = self.0[x as usize];
            if p == x {
                return x;
            }
            let g = self.0[p as usize];
            self.0[x as usize] = g;
            x = g;
        }
    }

    pub fn join(&mut self, a: u32, b: u32) {
        let (ra, rb) = (self.find(a), self.find(b));
        let (lo, hi) = (ra.min(rb), ra.max(rb));
        self.0[hi as usize] = lo;
    }
}

/// The unit numbered 0 in both forms: the outside.
pub const OUTSIDE: u32 = 0;

/// What both forms give the labelling walk.
pub trait Root {
    /// A region's kind; an empty region's unit, before joins.
    fn locate(&self, r: R) -> Loc;
    /// The disjoint sets of the units, joined.
    fn sets(&self) -> &Sets;
    /// Units made, the outside included.
    fn units(&self) -> usize;
}

// ---------------------------------------------------------------------------------------
// Dense: analysis.rs's form.

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

pub struct Dense {
    axes: [Axis; 3],
    keys: BTreeSet<R>,
    cells: BTreeMap<[usize; 3], u32>,
    sets: Sets,
}

impl Dense {
    /// `None` where `analysis.rs` refuses as `TooSpread`.
    pub fn new(keys: &BTreeSet<R>, conn: Conn) -> Option<Self> {
        let axis = |k: usize| Axis::new(&keys.iter().map(|c| c[k]).collect());
        let axes = [axis(0), axis(1), axis(2)];
        let n = axes.iter().try_fold(1_usize, |n, a| n.checked_mul(a.0.len()));
        if n.is_none_or(|n| n > ROOT_CELLS) {
            return None;
        }
        let mut d = Self { axes, keys: keys.clone(), cells: BTreeMap::new(), sets: Sets::default() };
        d.sets.add(); // the outside
        d.walk(conn);
        Some(d)
    }

    pub fn cells(&self) -> usize {
        self.axes.iter().map(|a| a.0.len()).product()
    }

    fn kind(&mut self, c: [usize; 3]) -> Loc {
        if (0..3).any(|k| self.axes[k].unbounded(c[k])) {
            return Loc::Outside;
        }
        let r = [0, 1, 2].map(|k| self.axes[k].singleton(c[k]));
        if let [Some(x), Some(y), Some(z)] = r
            && self.keys.contains(&[x, y, z])
        {
            return Loc::Key;
        }
        let sets = &mut self.sets;
        Loc::Empty(*self.cells.entry(c).or_insert_with(|| sets.add()))
    }

    fn walk(&mut self, conn: Conn) {
        let dims = self.axes.each_ref().map(|a| a.0.len());
        let forward = conn.forward();
        for z in 0..dims[2] {
            for y in 0..dims[1] {
                for x in 0..dims[0] {
                    let c = [x, y, z];
                    let kc = self.kind(c);
                    for e in &forward {
                        let d = [0, 1, 2].map(|k| c[k].checked_add_signed(e[k] as isize));
                        let [Some(dx), Some(dy), Some(dz)] = d else { continue };
                        if dx >= dims[0] || dy >= dims[1] || dz >= dims[2] {
                            continue;
                        }
                        let kd = self.kind([dx, dy, dz]);
                        let u = |k: Loc| match k {
                            Loc::Outside => Some(OUTSIDE),
                            Loc::Empty(u) => Some(u),
                            Loc::Key => None,
                        };
                        if let (Some(a), Some(b)) = (u(kc), u(kd)) {
                            self.sets.join(a, b);
                        }
                    }
                }
            }
        }
    }
}

impl Root for Dense {
    fn locate(&self, r: R) -> Loc {
        let c = [0, 1, 2].map(|k| self.axes[k].find(r[k]));
        if (0..3).any(|k| self.axes[k].unbounded(c[k])) {
            Loc::Outside
        } else if self.keys.contains(&r) {
            Loc::Key
        } else {
            Loc::Empty(self.cells[&c])
        }
    }

    fn sets(&self) -> &Sets {
        &self.sets
    }

    fn units(&self) -> usize {
        self.sets.0.len()
    }
}

// ---------------------------------------------------------------------------------------
// Sparse: boxes of empty space.

/// A run of x in a row, and its unit.
#[derive(Debug, Clone, Copy)]
struct Run {
    x0: i64,
    x1: i64,
    unit: u32,
}

#[derive(Debug, Clone)]
struct Row {
    y0: i64,
    y1: i64,
    runs: Vec<Run>,
}

#[derive(Debug, Clone)]
struct Slab {
    z0: i64,
    z1: i64,
    rows: Vec<Row>,
}

pub struct Sparse {
    lo: R,
    hi: R,
    keys: BTreeSet<R>,
    slabs: Vec<Slab>,
    sets: Sets,
}

/// The gap between two closed intervals: 0 if they overlap, 1 if they touch, more if not.
fn gap(a0: i64, a1: i64, b0: i64, b1: i64) -> i64 {
    if a1 < b0 { b0 - a1 } else if b1 < a0 { a0 - b1 } else { 0 }
}

impl Sparse {
    pub fn new(keys: &BTreeSet<R>, conn: Conn) -> Self {
        let mut s = Self { lo: [0; 3], hi: [0; 3], keys: keys.clone(), slabs: vec![], sets: Sets::default() };
        s.sets.add(); // the outside
        if keys.is_empty() {
            return s;
        }
        for k in 0..3 {
            s.lo[k] = keys.iter().map(|c| c[k]).min().unwrap() - 1;
            s.hi[k] = keys.iter().map(|c| c[k]).max().unwrap() + 1;
        }
        s.build();
        s.join(conn);
        s
    }

    pub fn boxes(&self) -> usize {
        self.slabs.iter().flat_map(|s| &s.rows).map(|r| r.runs.len()).sum()
    }

    fn build(&mut self) {
        // The keys by z, then y, then x: the set's own order is x first, so regroup.
        let mut by_z: BTreeMap<i64, BTreeMap<i64, Vec<i64>>> = BTreeMap::new();
        for &[x, y, z] in &self.keys {
            by_z.entry(z).or_default().entry(y).or_default().push(x);
        }
        let (lo, hi) = (self.lo, self.hi);
        let mut z = lo[2];
        let mut slabs = vec![];
        let mut sets = std::mem::take(&mut self.sets);
        let whole = |sets: &mut Sets, y0, y1| Row { y0, y1, runs: vec![Run { x0: lo[0], x1: hi[0], unit: sets.add() }] };
        for (&kz, rows) in &by_z {
            if z < kz {
                slabs.push(Slab { z0: z, z1: kz - 1, rows: vec![whole(&mut sets, lo[1], hi[1])] });
            }
            let mut out = vec![];
            let mut y = lo[1];
            for (&ky, xs) in rows {
                if y < ky {
                    out.push(whole(&mut sets, y, ky - 1));
                }
                let mut xs = xs.clone();
                xs.sort_unstable();
                let mut runs = vec![];
                let mut x = lo[0];
                for kx in xs {
                    if x < kx {
                        runs.push(Run { x0: x, x1: kx - 1, unit: sets.add() });
                    }
                    x = kx + 1;
                }
                runs.push(Run { x0: x, x1: hi[0], unit: sets.add() });
                out.push(Row { y0: ky, y1: ky, runs });
                y = ky + 1;
            }
            out.push(whole(&mut sets, y, hi[1]));
            slabs.push(Slab { z0: kz, z1: kz, rows: out });
            z = kz + 1;
        }
        slabs.push(Slab { z0: z, z1: hi[2], rows: vec![whole(&mut sets, lo[1], hi[1])] });
        self.slabs = slabs;
        self.sets = sets;
    }

    fn join(&mut self, conn: Conn) {
        let reach = conn.reach();
        let (lo, hi) = (self.lo, self.hi);
        let mut pairs = vec![];
        for (si, s) in self.slabs.iter().enumerate() {
            // The rim touches the outside.
            for row in &s.rows {
                for r in &row.runs {
                    if s.z0 == lo[2] || s.z1 == hi[2] || row.y0 == lo[1] || row.y1 == hi[1] || r.x0 == lo[0] || r.x1 == hi[0] {
                        pairs.push((OUTSIDE, r.unit));
                    }
                }
            }
            // Rows of one slab, one above the other.
            for w in s.rows.windows(2) {
                touching(&w[0], &w[1], 1, reach, &mut pairs);
            }
            // Rows of the next slab.
            if let Some(t) = self.slabs.get(si + 1) {
                let mut j = 0;
                for a in &s.rows {
                    while j < t.rows.len() && t.rows[j].y1 < a.y0 - 1 {
                        j += 1;
                    }
                    let mut k = j;
                    while k < t.rows.len() && t.rows[k].y0 <= a.y1 + 1 {
                        let dy = gap(a.y0, a.y1, t.rows[k].y0, t.rows[k].y1);
                        touching(a, &t.rows[k], 1 + dy, reach, &mut pairs);
                        k += 1;
                    }
                }
            }
        }
        for (a, b) in pairs {
            self.sets.join(a, b);
        }
    }

    fn row_at(&self, r: R) -> Option<&Row> {
        let s = &self.slabs[self.slabs.partition_point(|s| s.z1 < r[2])];
        Some(&s.rows[s.rows.partition_point(|w| w.y1 < r[1])])
    }
}

/// Joins the runs of two rows whose regions differ by `apart` coordinates so far.
fn touching(a: &Row, b: &Row, apart: i64, reach: i64, pairs: &mut Vec<(u32, u32)>) {
    if apart > reach {
        return;
    }
    let tol = (reach - apart).min(1);
    for r in &a.runs {
        let first = b.runs.partition_point(|q| q.x1 < r.x0 - tol);
        for q in &b.runs[first..] {
            if q.x0 > r.x1 + tol {
                break;
            }
            pairs.push((r.unit, q.unit));
        }
    }
}

impl Root for Sparse {
    fn locate(&self, r: R) -> Loc {
        if self.keys.is_empty() || (0..3).any(|k| r[k] < self.lo[k] || r[k] > self.hi[k]) {
            return Loc::Outside;
        }
        let row = self.row_at(r).expect("inside the box");
        let i = row.runs.partition_point(|q| q.x1 < r[0]);
        match row.runs.get(i) {
            Some(q) if q.x0 <= r[0] => Loc::Empty(q.unit),
            _ => Loc::Key,
        }
    }

    fn sets(&self) -> &Sets {
        &self.sets
    }

    fn units(&self) -> usize {
        self.sets.0.len()
    }
}

// ---------------------------------------------------------------------------------------
// Comparing the two.

/// A region's component under a form: the outside's, a key, or an empty component by its
/// least unit (so by the order the form made it).
pub fn component(f: &impl Root, sets: &mut Sets, r: R) -> Loc {
    match f.locate(r) {
        Loc::Key => Loc::Key,
        Loc::Outside => Loc::Outside,
        Loc::Empty(u) => match sets.find(u) {
            OUTSIDE => Loc::Outside,
            c => Loc::Empty(c),
        },
    }
}

/// Regions that between them meet every cell of both forms: each dense cell's two ends
/// (an unbounded end taken just past the keys), and each key's neighbours.
pub fn probes(keys: &BTreeSet<R>) -> Vec<R> {
    if keys.is_empty() {
        return vec![[0; 3]];
    }
    let axes: [Axis; 3] = [0, 1, 2].map(|k| Axis::new(&keys.iter().map(|c| c[k]).collect()));
    let ends = |a: &Axis| -> Vec<i64> {
        let n = a.0.len();
        let mut v = vec![];
        for (i, (lo, hi)) in a.0.iter().enumerate() {
            if i == 0 {
                v.push(*hi);
            } else if i + 1 == n {
                v.push(*lo);
            } else {
                v.push(*lo);
                v.push(*hi);
            }
        }
        v.sort_unstable();
        v.dedup();
        v
    };
    let [xs, ys, zs] = axes.each_ref().map(ends);
    let mut out = vec![];
    for &z in &zs {
        for &y in &ys {
            for &x in &xs {
                out.push([x, y, z]);
            }
        }
    }
    out
}
