//! What every candidate shares: a simplified topology, its edits, `Remap`, and the remap of
//! one field's values.
//!
//! The tree is simplified to leaf-sized regions (8³ voxels) in a `BTreeMap`: a region is a
//! leaf, a tile, or absent (the background). The questions are about the API, so the four
//! levels of ARCHITECTURE.md §4.1 are left out.

use std::collections::BTreeMap;

use rayon::prelude::*;

/// Voxels along one edge of a leaf.
pub const LEAF_DIM: i32 = 8;
/// Voxels in a leaf.
pub const LEAF_VOXELS: usize = 512;

/// A leaf-sized region: a voxel coordinate divided by `LEAF_DIM`, rounded down.
pub type Region = [i32; 3];
/// A voxel coordinate.
pub type Voxel = [i32; 3];

/// The region holding a voxel.
pub fn region_of(v: Voxel) -> Region {
    v.map(|c| c.div_euclid(LEAF_DIM))
}

/// A voxel's index inside its leaf, x fastest.
pub fn local_index(v: Voxel) -> usize {
    let l = v.map(|c| c.rem_euclid(LEAF_DIM) as usize);
    l[0] + 8 * l[1] + 64 * l[2]
}

/// The voxel at index `i` of the leaf in region `r`.
pub fn voxel_of(r: Region, i: usize) -> Voxel {
    let l = [i % 8, (i / 8) % 8, i / 64].map(|c| c as i32);
    [r[0] * LEAF_DIM + l[0], r[1] * LEAF_DIM + l[1], r[2] * LEAF_DIM + l[2]]
}

/// The Morton key of a region; leaves and tiles are numbered in this order.
pub fn morton(r: Region) -> u128 {
    let spread = |c: i32| {
        let x = (c as u32) ^ 0x8000_0000;
        (0..32).fold(0u128, |acc, b| acc | (u128::from((x >> b) & 1) << (3 * b)))
    };
    spread(r[0]) | (spread(r[1]) << 1) | (spread(r[2]) << 2)
}

/// A value a field can hold, compared by bits.
pub trait Value: Copy + Send + Sync + 'static {
    /// Whether two values have the same bits.
    fn same_bits(&self, other: &Self) -> bool;
}

impl Value for f32 {
    fn same_bits(&self, other: &Self) -> bool {
        self.to_bits() == other.to_bits()
    }
}

impl Value for [f32; 3] {
    fn same_bits(&self, other: &Self) -> bool {
        self.iter().zip(other).all(|(a, b)| a.to_bits() == b.to_bits())
    }
}

impl Value for u8 {
    fn same_bits(&self, other: &Self) -> bool {
        self == other
    }
}

/// Where a new tile's value comes from (B-008).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileSource {
    /// An old tile, by its old number.
    Tile(u32),
    /// An old leaf merged into the tile, by its old number.
    Merged(u32),
    /// The background.
    Background,
}

/// Where a new leaf's values come from (B-008).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeafSource {
    /// An old leaf, by its old number.
    Leaf(u32),
    /// A tile split into the leaf: every voxel takes the tile's value.
    Split(TileSource),
    /// Activated from the background.
    Background,
}

/// What a region holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// A leaf, by number.
    Leaf(u32),
    /// A tile, by number.
    Tile(u32),
    /// Nothing: the background.
    Background,
}

/// The simplified topology: leaves and tiles in Morton order.
#[derive(Debug, Clone, Default)]
pub struct Shape {
    /// Leaf regions, in Morton order; a leaf's number is its position.
    pub leaves: Vec<Region>,
    /// Tile regions, in Morton order; a tile's number is its position.
    pub tiles: Vec<Region>,
    slots: BTreeMap<Region, Slot>,
}

impl Shape {
    fn from_lists(leaves: Vec<Region>, tiles: Vec<Region>) -> Self {
        let mut slots = BTreeMap::new();
        for (i, r) in leaves.iter().enumerate() {
            slots.insert(*r, Slot::Leaf(i as u32));
        }
        for (i, r) in tiles.iter().enumerate() {
            slots.insert(*r, Slot::Tile(i as u32));
        }
        Self { leaves, tiles, slots }
    }

    /// What the region holds.
    pub fn slot(&self, r: Region) -> Slot {
        self.slots.get(&r).copied().unwrap_or(Slot::Background)
    }
}

/// From an old shape's leaf and tile numbers to a new shape's, with where each new value
/// comes from.
#[derive(Debug, Clone)]
pub struct Remap {
    /// Leaves in the old shape.
    pub old_leaves: usize,
    /// Tiles in the old shape.
    pub old_tiles: usize,
    /// One per new leaf.
    pub leaves: Vec<LeafSource>,
    /// One per new tile.
    pub tiles: Vec<TileSource>,
}

/// A topology edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// Ensure the voxel's region is a leaf: a tile is split, the background activated.
    Activate(Voxel),
    /// Split a tile into a leaf.
    Split(Region),
    /// Merge a leaf into a tile.
    Merge(Region),
    /// Make an absent region a tile, valued from the background.
    AddTile(Region),
    /// Return a leaf or tile to the background.
    Remove(Region),
}

/// Every error of the spike.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpikeError {
    /// `Split` on a region that is not a tile.
    NotATile(Region),
    /// `Merge` on a region that is not a leaf.
    NotALeaf(Region),
    /// `AddTile` on a region already a leaf or tile.
    Occupied(Region),
    /// `Remove` on an absent region.
    Absent(Region),
    /// A merge rule refused a leaf's values, by old leaf number.
    NotUniform(u32),
    /// A field used with a topology it was not made or remapped for.
    StaleField,
    /// A field whose value counts do not match the remap.
    WrongShape,
    /// A field handle unknown to the set, or of another type.
    UnknownField,
    /// The same field asked for as both read and written.
    SameField,
}

#[derive(Debug, Clone, Copy)]
enum Pending {
    Leaf(LeafSource),
    Tile(TileSource),
}

/// Applies edits to a shape, giving the new shape and the remap to it.
pub fn apply(shape: &Shape, ops: &[Op]) -> Result<(Shape, Remap), SpikeError> {
    let mut slots: BTreeMap<Region, Pending> = BTreeMap::new();
    for (i, r) in shape.leaves.iter().enumerate() {
        slots.insert(*r, Pending::Leaf(LeafSource::Leaf(i as u32)));
    }
    for (i, r) in shape.tiles.iter().enumerate() {
        slots.insert(*r, Pending::Tile(TileSource::Tile(i as u32)));
    }
    for op in ops {
        apply_op(&mut slots, *op)?;
    }
    let mut leaves: Vec<(Region, LeafSource)> = Vec::new();
    let mut tiles: Vec<(Region, TileSource)> = Vec::new();
    for (r, p) in slots {
        match p {
            Pending::Leaf(s) => leaves.push((r, s)),
            Pending::Tile(s) => tiles.push((r, s)),
        }
    }
    leaves.sort_by_key(|(r, _)| morton(*r));
    tiles.sort_by_key(|(r, _)| morton(*r));
    let remap = Remap {
        old_leaves: shape.leaves.len(),
        old_tiles: shape.tiles.len(),
        leaves: leaves.iter().map(|(_, s)| *s).collect(),
        tiles: tiles.iter().map(|(_, s)| *s).collect(),
    };
    let new = Shape::from_lists(
        leaves.into_iter().map(|(r, _)| r).collect(),
        tiles.into_iter().map(|(r, _)| r).collect(),
    );
    Ok((new, remap))
}

fn apply_op(slots: &mut BTreeMap<Region, Pending>, op: Op) -> Result<(), SpikeError> {
    match op {
        Op::Activate(v) => {
            let r = region_of(v);
            let next = match slots.get(&r) {
                Some(Pending::Leaf(_)) => None,
                Some(Pending::Tile(t)) => Some(Pending::Leaf(LeafSource::Split(*t))),
                None => Some(Pending::Leaf(LeafSource::Background)),
            };
            if let Some(p) = next {
                slots.insert(r, p);
            }
        }
        Op::Split(r) => match slots.get(&r) {
            Some(Pending::Tile(t)) => {
                let t = *t;
                slots.insert(r, Pending::Leaf(LeafSource::Split(t)));
            }
            Some(Pending::Leaf(_)) | None => return Err(SpikeError::NotATile(r)),
        },
        Op::Merge(r) => match slots.get(&r) {
            Some(Pending::Leaf(s)) => {
                let t = match *s {
                    LeafSource::Leaf(i) => TileSource::Merged(i),
                    LeafSource::Split(t) => t,
                    LeafSource::Background => TileSource::Background,
                };
                slots.insert(r, Pending::Tile(t));
            }
            Some(Pending::Tile(_)) | None => return Err(SpikeError::NotALeaf(r)),
        },
        Op::AddTile(r) => {
            if slots.contains_key(&r) {
                return Err(SpikeError::Occupied(r));
            }
            slots.insert(r, Pending::Tile(TileSource::Background));
        }
        Op::Remove(r) => {
            if slots.remove(&r).is_none() {
                return Err(SpikeError::Absent(r));
            }
        }
    }
    Ok(())
}

/// One field's values over a shape (ARCHITECTURE.md §4.2).
#[derive(Debug, Clone)]
pub struct FieldData<T> {
    /// Leaf count × 512, in leaf order.
    pub leaf_values: Vec<T>,
    /// One per tile.
    pub tile_values: Vec<T>,
    /// The value outside the tree.
    pub background: T,
}

impl<T: Value> FieldData<T> {
    /// Every value the background.
    pub fn new(shape: &Shape, background: T) -> Self {
        Self {
            leaf_values: vec![background; shape.leaves.len() * LEAF_VOXELS],
            tile_values: vec![background; shape.tiles.len()],
            background,
        }
    }

    /// One leaf's values.
    pub fn leaf(&self, i: u32) -> &[T] {
        let s = i as usize * LEAF_VOXELS;
        &self.leaf_values[s..s + LEAF_VOXELS]
    }

    /// One leaf's values, to write.
    pub fn leaf_mut(&mut self, i: u32) -> &mut [T] {
        let s = i as usize * LEAF_VOXELS;
        &mut self.leaf_values[s..s + LEAF_VOXELS]
    }

    /// The value at a voxel: its leaf's, its tile's, or the background.
    pub fn value_at(&self, shape: &Shape, v: Voxel) -> T {
        match shape.slot(region_of(v)) {
            Slot::Leaf(i) => self.leaf(i)[local_index(v)],
            Slot::Tile(j) => self.tile_values[j as usize],
            Slot::Background => self.background,
        }
    }
}

/// How a field's values become a tile's value when its leaf is merged: `None` refuses.
pub type MergeRule<T> = dyn Fn(&[T]) -> Option<T> + Send + Sync;

/// Candidate M1: merge only a leaf whose values all have the same bits.
pub fn uniform<T: Value>(values: &[T]) -> Option<T> {
    let first = *values.first()?;
    values.iter().all(|v| v.same_bits(&first)).then_some(first)
}

/// Remaps one field's values. Merged values are worked out first, in leaf order, so the
/// error reported is the same whatever the thread count; the copy is then leaf-parallel.
pub fn remap_data<T: Value>(
    old: &FieldData<T>,
    r: &Remap,
    merge: &MergeRule<T>,
) -> Result<FieldData<T>, SpikeError> {
    if old.leaf_values.len() != r.old_leaves * LEAF_VOXELS || old.tile_values.len() != r.old_tiles
    {
        return Err(SpikeError::WrongShape);
    }
    let mut merged: BTreeMap<u32, T> = BTreeMap::new();
    let merged_from = r.tiles.iter().copied().chain(r.leaves.iter().filter_map(|s| match s {
        LeafSource::Split(t) => Some(*t),
        LeafSource::Leaf(_) | LeafSource::Background => None,
    }));
    for t in merged_from {
        if let TileSource::Merged(i) = t {
            if !merged.contains_key(&i) {
                merged.insert(i, merge(old.leaf(i)).ok_or(SpikeError::NotUniform(i))?);
            }
        }
    }
    let tile_value = |t: TileSource| match t {
        TileSource::Tile(j) => old.tile_values[j as usize],
        TileSource::Merged(i) => merged[&i],
        TileSource::Background => old.background,
    };
    let mut leaf_values = vec![old.background; r.leaves.len() * LEAF_VOXELS];
    leaf_values
        .par_chunks_mut(LEAF_VOXELS)
        .zip(r.leaves.par_iter())
        .for_each(|(out, s)| match *s {
            LeafSource::Leaf(i) => out.copy_from_slice(old.leaf(i)),
            LeafSource::Split(t) => out.fill(tile_value(t)),
            LeafSource::Background => out.fill(old.background),
        });
    Ok(FieldData {
        leaf_values,
        tile_values: r.tiles.iter().map(|t| tile_value(*t)).collect(),
        background: old.background,
    })
}

/// Writes each leaf of `out` in parallel, one 512-value chunk per leaf (ARCHITECTURE.md §4.4).
pub fn par_update<U: Value>(out: &mut FieldData<U>, f: impl Fn(u32, &mut [U]) + Sync) {
    out.leaf_values
        .par_chunks_mut(LEAF_VOXELS)
        .enumerate()
        .for_each(|(i, chunk)| f(i as u32, chunk));
}
