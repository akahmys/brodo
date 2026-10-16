//! Spike for B-010 (ROADMAP.md §3): merging leaves and tiles into a tile above leaf level.
//!
//! A two-level simplification of brodo's tree: leaf-sized regions (8³ voxels), each a leaf,
//! a leaf-sized tile or nothing, grouped in blocks of `dim`³ regions, each of which may be a
//! block tile instead (`dim` = 16 is brodo's lower node, 128³ voxels). The candidates:
//!
//! - what a block merge accepts: only tiles (S1, `Merging::Stepwise`) or leaves and tiles at
//!   once (S2, `Merging::AtOnce`); regions holding nothing refused (B-a) or merged as
//!   background (B-b);
//! - how `Remap` names a merged tile's sources: a `Vec` per tile (R1, `RemapR1`) or ranges
//!   into one shared list (R2, `RemapR2`);
//! - what the merge rule sees: every voxel's value (M-a, `Rule::Voxels`) or values with
//!   voxel counts (M-b, `Rule::Counted`).
//!
//! Sources are found as brodo's commit finds them: by locating each new leaf and tile in the
//! old topology. The report is `REPORT.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use rayon::prelude::*;

/// Voxels along a leaf's edge.
pub const LEAF_DIM: i32 = 8;
/// Voxels in a leaf.
pub const LEAF_VOXELS: usize = 512;

/// A leaf-sized region: a voxel divided by 8, rounded down.
pub type Region = [i32; 3];
/// A block: a region divided by `dim`, rounded down.
pub type Block = [i32; 3];
/// A voxel.
pub type Voxel = [i32; 3];

pub fn region_of(v: Voxel) -> Region {
    v.map(|c| c.div_euclid(LEAF_DIM))
}

pub fn block_of(r: Region, dim: i32) -> Block {
    r.map(|c| c.div_euclid(dim))
}

pub fn local_index(v: Voxel) -> usize {
    let l = v.map(|c| c.rem_euclid(LEAF_DIM) as usize);
    l[0] + 8 * l[1] + 64 * l[2]
}

pub fn voxel_of(r: Region, i: usize) -> Voxel {
    let l = [i % 8, i / 8 % 8, i / 64].map(|c| c as i32);
    [r[0] * 8 + l[0], r[1] * 8 + l[1], r[2] * 8 + l[2]]
}

/// The regions of a block, x fastest.
pub fn children(b: Block, dim: i32) -> impl Iterator<Item = Region> {
    let o = b.map(|c| c * dim);
    (0..dim).flat_map(move |z| {
        (0..dim).flat_map(move |y| (0..dim).map(move |x| [o[0] + x, o[1] + y, o[2] + z]))
    })
}

/// A value a field can hold, compared by bits.
pub trait Value: Copy + Send + Sync + 'static + std::fmt::Debug {
    fn same_bits(&self, other: &Self) -> bool;
}
impl Value for f64 {
    fn same_bits(&self, other: &Self) -> bool {
        self.to_bits() == other.to_bits()
    }
}
impl Value for f32 {
    fn same_bits(&self, other: &Self) -> bool {
        self.to_bits() == other.to_bits()
    }
}
impl Value for u8 {
    fn same_bits(&self, other: &Self) -> bool {
        self == other
    }
}

/// A tile: leaf-sized, or a whole block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TileKey {
    Leaf(Region),
    Block(Block),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    Leaf,
    Tile,
}

/// Which regions are leaves or tiles, and which blocks are tiles.
#[derive(Clone, Debug)]
pub struct Shape {
    pub dim: i32,
    slots: BTreeMap<Region, Slot>,
    blocks: BTreeSet<Block>,
}

/// S1 or S2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Merging {
    /// S1: a block merges only when every region it holds is a tile.
    Stepwise,
    /// S2: a block merges leaves and tiles at once.
    AtOnce,
}

/// B-a or B-b.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackgroundInMerge {
    /// B-a: a block holding an empty region does not merge.
    Refuse,
    /// B-b: empty regions merge as the background; they become part of the tile.
    Include,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    pub merging: Merging,
    pub background: BackgroundInMerge,
}

pub const POLICIES: [Policy; 4] = [
    Policy { merging: Merging::Stepwise, background: BackgroundInMerge::Refuse },
    Policy { merging: Merging::Stepwise, background: BackgroundInMerge::Include },
    Policy { merging: Merging::AtOnce, background: BackgroundInMerge::Refuse },
    Policy { merging: Merging::AtOnce, background: BackgroundInMerge::Include },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// A region becomes a leaf where nothing is; inside a tile nothing changes.
    Activate(Region),
    AddTile(Region),
    AddBlockTile(Block),
    SplitTile(Region),
    /// A block tile becomes `dim`³ leaf-sized tiles.
    SplitBlock(Block),
    MergeLeaf(Region),
    MergeBlock(Block),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpikeError {
    Occupied,
    NotATile,
    NotALeaf,
    /// S1: the block holds a leaf.
    NotAllTiles(Block),
    /// B-a: the block holds an empty region.
    HoldsBackground(Block),
    /// The block holds nothing at all.
    Empty(Block),
    /// A merge rule refused the values of a new tile, by number.
    Refused(u32),
    WrongShape,
}

impl Shape {
    pub fn new(dim: i32) -> Self {
        Self { dim, slots: BTreeMap::new(), blocks: BTreeSet::new() }
    }

    fn in_block_tile(&self, r: Region) -> bool {
        self.blocks.contains(&block_of(r, self.dim))
    }

    pub fn apply(&self, ops: &[Op], p: Policy) -> Result<Shape, SpikeError> {
        let mut s = self.clone();
        for op in ops {
            s.apply_op(*op, p)?;
        }
        Ok(s)
    }

    fn apply_op(&mut self, op: Op, p: Policy) -> Result<(), SpikeError> {
        match op {
            Op::Activate(r) => {
                if !self.in_block_tile(r) {
                    self.slots.entry(r).or_insert(Slot::Leaf);
                }
            }
            Op::AddTile(r) => {
                if self.in_block_tile(r) || self.slots.contains_key(&r) {
                    return Err(SpikeError::Occupied);
                }
                self.slots.insert(r, Slot::Tile);
            }
            Op::AddBlockTile(b) => {
                if self.blocks.contains(&b) || children(b, self.dim).any(|c| self.slots.contains_key(&c)) {
                    return Err(SpikeError::Occupied);
                }
                self.blocks.insert(b);
            }
            Op::SplitTile(r) => match self.slots.get(&r) {
                Some(Slot::Tile) => {
                    self.slots.insert(r, Slot::Leaf);
                }
                Some(Slot::Leaf) | None => return Err(SpikeError::NotATile),
            },
            Op::SplitBlock(b) => {
                if !self.blocks.remove(&b) {
                    return Err(SpikeError::NotATile);
                }
                for c in children(b, self.dim) {
                    self.slots.insert(c, Slot::Tile);
                }
            }
            Op::MergeLeaf(r) => match self.slots.get(&r) {
                Some(Slot::Leaf) => {
                    self.slots.insert(r, Slot::Tile);
                }
                Some(Slot::Tile) | None => return Err(SpikeError::NotALeaf),
            },
            Op::MergeBlock(b) => self.merge_block(b, p)?,
        }
        Ok(())
    }

    fn merge_block(&mut self, b: Block, p: Policy) -> Result<(), SpikeError> {
        if self.blocks.contains(&b) {
            return Err(SpikeError::Occupied);
        }
        let present: Vec<(Region, Slot)> =
            children(b, self.dim).filter_map(|c| self.slots.get(&c).map(|s| (c, *s))).collect();
        if present.is_empty() {
            return Err(SpikeError::Empty(b));
        }
        if p.merging == Merging::Stepwise && present.iter().any(|(_, s)| *s == Slot::Leaf) {
            return Err(SpikeError::NotAllTiles(b));
        }
        let all = (self.dim * self.dim * self.dim) as usize;
        if present.len() < all && p.background == BackgroundInMerge::Refuse {
            return Err(SpikeError::HoldsBackground(b));
        }
        for (c, _) in present {
            self.slots.remove(&c);
        }
        self.blocks.insert(b);
        Ok(())
    }
}

/// What holds a region in a topology.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Located {
    Leaf(u32),
    Tile(u32),
    Background,
}

/// A shape with its leaves and tiles numbered.
#[derive(Clone, Debug)]
pub struct Topology {
    shape: Shape,
    leaves: Vec<Region>,
    leaf_no: BTreeMap<Region, u32>,
    tiles: Vec<TileKey>,
    tile_no: BTreeMap<TileKey, u32>,
}

impl Topology {
    pub fn new(dim: i32) -> Self {
        Self::from_shape(Shape::new(dim))
    }

    fn from_shape(shape: Shape) -> Self {
        let leaves: Vec<Region> =
            shape.slots.iter().filter(|(_, s)| **s == Slot::Leaf).map(|(r, _)| *r).collect();
        let mut tiles: Vec<TileKey> = shape
            .slots
            .iter()
            .filter(|(_, s)| **s == Slot::Tile)
            .map(|(r, _)| TileKey::Leaf(*r))
            .chain(shape.blocks.iter().map(|b| TileKey::Block(*b)))
            .collect();
        tiles.sort_unstable();
        let leaf_no = leaves.iter().zip(0..).map(|(r, n)| (*r, n)).collect();
        let tile_no = tiles.iter().zip(0..).map(|(k, n)| (*k, n)).collect();
        Self { shape, leaves, leaf_no, tiles, tile_no }
    }

    pub fn shape(&self) -> &Shape {
        &self.shape
    }
    pub fn leaves(&self) -> &[Region] {
        &self.leaves
    }
    pub fn tiles(&self) -> &[TileKey] {
        &self.tiles
    }

    pub fn locate(&self, r: Region) -> Located {
        if let Some(t) = self.tile_no.get(&TileKey::Block(block_of(r, self.shape.dim))) {
            return Located::Tile(*t);
        }
        match self.shape.slots.get(&r) {
            Some(Slot::Leaf) => Located::Leaf(self.leaf_no[&r]),
            Some(Slot::Tile) => Located::Tile(self.tile_no[&TileKey::Leaf(r)]),
            None => Located::Background,
        }
    }
}

/// One source of a merged tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Leaf(u32),
    Tile(u32),
    /// This many empty leaf-sized regions, merged as the background (B-b).
    Background(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeafSource {
    Leaf(u32),
    /// From the old tile it was split out of.
    Split(u32),
    Background,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TileSource<P> {
    Tile(u32),
    Merged(P),
    Background,
}

/// What a remap says of one new tile, whichever the representation.
pub enum TileView<'a> {
    Tile(u32),
    Merged(&'a [Part]),
    Background,
}

pub trait Sources: Sync {
    fn leaf_sources(&self) -> &[LeafSource];
    fn tile_count(&self) -> usize;
    fn tile(&self, i: usize) -> TileView<'_>;
    /// Heap and inline bytes the remap holds.
    fn bytes(&self) -> usize;
}

/// R1: a `Vec` of parts per merged tile.
pub struct RemapR1 {
    pub leaves: Vec<LeafSource>,
    pub tiles: Vec<TileSource<Vec<Part>>>,
}

/// R2: one list of parts; a merged tile holds its range.
pub struct RemapR2 {
    pub leaves: Vec<LeafSource>,
    pub tiles: Vec<TileSource<Range<u32>>>,
    pub parts: Vec<Part>,
}

impl Sources for RemapR1 {
    fn leaf_sources(&self) -> &[LeafSource] {
        &self.leaves
    }
    fn tile_count(&self) -> usize {
        self.tiles.len()
    }
    fn tile(&self, i: usize) -> TileView<'_> {
        match &self.tiles[i] {
            TileSource::Tile(t) => TileView::Tile(*t),
            TileSource::Merged(p) => TileView::Merged(p),
            TileSource::Background => TileView::Background,
        }
    }
    fn bytes(&self) -> usize {
        let parts: usize = self
            .tiles
            .iter()
            .map(|t| match t {
                TileSource::Merged(p) => p.capacity() * size_of::<Part>(),
                TileSource::Tile(_) | TileSource::Background => 0,
            })
            .sum();
        self.leaves.capacity() * size_of::<LeafSource>()
            + self.tiles.capacity() * size_of::<TileSource<Vec<Part>>>()
            + parts
    }
}

impl Sources for RemapR2 {
    fn leaf_sources(&self) -> &[LeafSource] {
        &self.leaves
    }
    fn tile_count(&self) -> usize {
        self.tiles.len()
    }
    fn tile(&self, i: usize) -> TileView<'_> {
        match &self.tiles[i] {
            TileSource::Tile(t) => TileView::Tile(*t),
            TileSource::Merged(r) => TileView::Merged(&self.parts[r.start as usize..r.end as usize]),
            TileSource::Background => TileView::Background,
        }
    }
    fn bytes(&self) -> usize {
        self.leaves.capacity() * size_of::<LeafSource>()
            + self.tiles.capacity() * size_of::<TileSource<Range<u32>>>()
            + self.parts.capacity() * size_of::<Part>()
    }
}

fn leaf_sources(old: &Topology, new: &Topology) -> Vec<LeafSource> {
    new.leaves
        .iter()
        .map(|r| match old.locate(*r) {
            Located::Leaf(n) => LeafSource::Leaf(n),
            Located::Tile(t) => LeafSource::Split(t),
            Located::Background => LeafSource::Background,
        })
        .collect()
}

/// Where a new tile's value comes from: the old tile where one stood, the parts it merges,
/// or the background. `push` receives the parts; the result says what to record.
fn tile_source(old: &Topology, key: TileKey, push: &mut impl FnMut(Part)) -> TileSource<()> {
    let dim = old.shape.dim;
    match key {
        TileKey::Leaf(r) => match old.locate(r) {
            Located::Tile(t) => TileSource::Tile(t),
            Located::Leaf(n) => {
                push(Part::Leaf(n));
                TileSource::Merged(())
            }
            Located::Background => TileSource::Background,
        },
        TileKey::Block(b) => {
            if let Some(t) = old.tile_no.get(&TileKey::Block(b)) {
                return TileSource::Tile(*t);
            }
            let (mut empty, mut any) = (0, false);
            for c in children(b, dim) {
                match old.locate(c) {
                    Located::Leaf(n) => {
                        push(Part::Leaf(n));
                        any = true;
                    }
                    Located::Tile(t) => {
                        push(Part::Tile(t));
                        any = true;
                    }
                    Located::Background => empty += 1,
                }
            }
            if !any {
                return TileSource::Background;
            }
            if empty > 0 {
                push(Part::Background(empty));
            }
            TileSource::Merged(())
        }
    }
}

pub fn commit_r1(old: &Topology, ops: &[Op], p: Policy) -> Result<(Topology, RemapR1), SpikeError> {
    let new = Topology::from_shape(old.shape.apply(ops, p)?);
    let tiles = new
        .tiles
        .iter()
        .map(|k| {
            let mut parts = Vec::new();
            match tile_source(old, *k, &mut |part| parts.push(part)) {
                TileSource::Tile(t) => TileSource::Tile(t),
                TileSource::Merged(()) => TileSource::Merged(parts),
                TileSource::Background => TileSource::Background,
            }
        })
        .collect();
    let remap = RemapR1 { leaves: leaf_sources(old, &new), tiles };
    Ok((new, remap))
}

pub fn commit_r2(old: &Topology, ops: &[Op], p: Policy) -> Result<(Topology, RemapR2), SpikeError> {
    let new = Topology::from_shape(old.shape.apply(ops, p)?);
    let mut parts = Vec::new();
    let tiles = new
        .tiles
        .iter()
        .map(|k| {
            let start = parts.len() as u32;
            match tile_source(old, *k, &mut |part| parts.push(part)) {
                TileSource::Tile(t) => TileSource::Tile(t),
                TileSource::Merged(()) => TileSource::Merged(start..parts.len() as u32),
                TileSource::Background => TileSource::Background,
            }
        })
        .collect();
    let remap = RemapR2 { leaves: leaf_sources(old, &new), tiles, parts };
    Ok((new, remap))
}

/// One field's values.
#[derive(Clone, Debug)]
pub struct FieldData<T> {
    pub leaf_values: Vec<T>,
    pub tile_values: Vec<T>,
    pub background: T,
}

impl<T: Value> FieldData<T> {
    pub fn new(t: &Topology, background: T) -> Self {
        Self {
            leaf_values: vec![background; t.leaves.len() * LEAF_VOXELS],
            tile_values: vec![background; t.tiles.len()],
            background,
        }
    }

    pub fn leaf(&self, n: u32) -> &[T] {
        let s = n as usize * LEAF_VOXELS;
        &self.leaf_values[s..s + LEAF_VOXELS]
    }

    pub fn leaf_mut(&mut self, n: u32) -> &mut [T] {
        let s = n as usize * LEAF_VOXELS;
        &mut self.leaf_values[s..s + LEAF_VOXELS]
    }

    pub fn value_at(&self, t: &Topology, v: Voxel) -> T {
        match t.locate(region_of(v)) {
            Located::Leaf(n) => self.leaf(n)[local_index(v)],
            Located::Tile(k) => self.tile_values[k as usize],
            Located::Background => self.background,
        }
    }
}

/// M-a: a rule on every voxel's value.
pub type VoxelRule<T> = dyn Fn(&[T]) -> Option<T> + Sync;
/// M-b: a rule on values with the number of voxels holding each.
pub type CountedRule<T> = dyn Fn(&[(T, u32)]) -> Option<T> + Sync;

#[derive(Clone, Copy)]
pub enum Rule<'a, T> {
    Voxels(&'a VoxelRule<T>),
    Counted(&'a CountedRule<T>),
}

fn merged_value<T: Value>(old: &FieldData<T>, parts: &[Part], rule: Rule<'_, T>) -> Option<T> {
    match rule {
        Rule::Voxels(f) => {
            let mut v = Vec::new();
            for p in parts {
                match *p {
                    Part::Leaf(n) => v.extend_from_slice(old.leaf(n)),
                    Part::Tile(t) => v.extend(std::iter::repeat_n(old.tile_values[t as usize], LEAF_VOXELS)),
                    Part::Background(k) => {
                        v.extend(std::iter::repeat_n(old.background, LEAF_VOXELS * k as usize))
                    }
                }
            }
            f(&v)
        }
        Rule::Counted(f) => {
            let mut v = Vec::new();
            for p in parts {
                match *p {
                    Part::Leaf(n) => v.extend(old.leaf(n).iter().map(|x| (*x, 1))),
                    Part::Tile(t) => v.push((old.tile_values[t as usize], LEAF_VOXELS as u32)),
                    Part::Background(k) => v.push((old.background, LEAF_VOXELS as u32 * k)),
                }
            }
            f(&v)
        }
    }
}

/// Remaps one field: merged tiles first, in tile order, so the error does not depend on the
/// thread count; then the leaves in parallel.
pub fn remap<T: Value>(
    old: &FieldData<T>,
    r: &impl Sources,
    rule: Rule<'_, T>,
) -> Result<FieldData<T>, SpikeError> {
    let tile_values = (0..r.tile_count())
        .map(|i| match r.tile(i) {
            TileView::Tile(t) => Ok(old.tile_values[t as usize]),
            TileView::Background => Ok(old.background),
            TileView::Merged(parts) => {
                merged_value(old, parts, rule).ok_or(SpikeError::Refused(i as u32))
            }
        })
        .collect::<Result<Vec<T>, SpikeError>>()?;
    let mut leaf_values = vec![old.background; r.leaf_sources().len() * LEAF_VOXELS];
    leaf_values.par_chunks_mut(LEAF_VOXELS).zip(r.leaf_sources().par_iter()).for_each(|(out, s)| {
        match *s {
            LeafSource::Leaf(n) => out.copy_from_slice(old.leaf(n)),
            LeafSource::Split(t) => out.fill(old.tile_values[t as usize]),
            LeafSource::Background => out.fill(old.background),
        }
    });
    Ok(FieldData { leaf_values, tile_values, background: old.background })
}

/// M-a, uniform: every voxel the same bits.
pub fn uniform_voxels<T: Value>(v: &[T]) -> Option<T> {
    let first = *v.first()?;
    v.iter().all(|x| x.same_bits(&first)).then_some(first)
}

/// M-b, uniform.
pub fn uniform_counted<T: Value>(v: &[(T, u32)]) -> Option<T> {
    let first = v.first()?.0;
    v.iter().all(|(x, _)| x.same_bits(&first)).then_some(first)
}

/// M-a, mean.
pub fn mean_voxels(v: &[f64]) -> Option<f64> {
    Some(v.iter().sum::<f64>() / v.len() as f64)
}

/// M-b, mean: weighted by voxel counts.
pub fn mean_counted(v: &[(f64, u32)]) -> Option<f64> {
    let n: f64 = v.iter().map(|(_, c)| f64::from(*c)).sum();
    Some(v.iter().map(|(x, c)| x * f64::from(*c)).sum::<f64>() / n)
}
