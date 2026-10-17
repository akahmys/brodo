//! The sparse tree (ARCHITECTURE.md §4.1, ADR-0013): a root map, internal nodes of 32³ and
//! 16³ children, and leaves of 8³ voxels. The tree holds topology only: which regions are
//! leaves, which are tiles, and which are the background. Values belong to fields (§4.2).

use std::collections::BTreeMap;

use crate::error::{NodeKind, TreeError};

/// A voxel coordinate.
pub type Coord = [i32; 3];

/// log2 of a leaf's edge, in voxels.
const LEAF_LOG2: u32 = 3;
/// log2 of a lower node's edge, in children.
const LOWER_DIM_LOG2: u32 = 4;
/// log2 of an upper node's edge, in children.
const UPPER_DIM_LOG2: u32 = 5;
/// log2 of a lower node's edge, in voxels.
const LOWER_SPAN_LOG2: u32 = LEAF_LOG2 + LOWER_DIM_LOG2;
/// log2 of an upper node's edge, in voxels.
const UPPER_SPAN_LOG2: u32 = LOWER_SPAN_LOG2 + UPPER_DIM_LOG2;

/// The corner of the aligned cube of edge `2^log2` holding `v`, rounding toward negative
/// infinity.
fn align(v: Coord, log2: u32) -> Coord {
    v.map(|c| (c >> log2) << log2)
}

/// The index of the child holding `v` in a node of `2^dim_log2` children per edge whose
/// children span `2^child_log2` voxels per edge, x fastest.
fn child_index(v: Coord, child_log2: u32, dim_log2: u32) -> usize {
    let mask = (1_u32 << dim_log2) - 1;
    let [x, y, z] = v.map(|c| (c.cast_unsigned() >> child_log2) & mask);
    to_usize(x | (y << dim_log2) | (z << (2 * dim_log2)))
}

/// The origin of child `i` of a node at `origin`, inverting `child_index`.
fn child_origin(origin: Coord, i: usize, child_log2: u32, dim_log2: u32) -> Coord {
    let mask = (1_usize << dim_log2) - 1;
    let at = [i & mask, (i >> dim_log2) & mask, i >> (2 * dim_log2)];
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        reason = "each part is below 32, the widest node's edge in children"
    )]
    let off = at.map(|c| (c as i32) << child_log2);
    [origin[0] + off[0], origin[1] + off[1], origin[2] + off[2]]
}

fn to_usize(i: u32) -> usize {
    // Lossless: brodo's targets have pointers of at least 32 bits (`TARGETS`).
    i as usize
}

/// A bit per child of a node.
#[derive(Debug, Clone)]
struct Mask<const W: usize>([u64; W]);

impl<const W: usize> Mask<W> {
    const EMPTY: Self = Self([0; W]);
    const FULL: Self = Self([u64::MAX; W]);

    fn get(&self, i: usize) -> bool {
        (self.0[i >> 6] >> (i & 63)) & 1 == 1
    }

    fn set(&mut self, i: usize) {
        self.0[i >> 6] |= 1 << (i & 63);
    }

    fn clear(&mut self, i: usize) {
        self.0[i >> 6] &= !(1 << (i & 63));
    }

    /// The number of set bits below bit `i`: the offset of child `i` among the children.
    fn count_before(&self, i: usize) -> usize {
        let w = i >> 6;
        let whole: u32 = self.0[..w].iter().map(|x| x.count_ones()).sum();
        let part = (self.0[w] & ((1_u64 << (i & 63)) - 1)).count_ones();
        to_usize(whole + part)
    }

    /// The set bits, lowest first.
    fn ones(&self) -> impl Iterator<Item = usize> + '_ {
        self.0.iter().enumerate().flat_map(|(w, word)| {
            let mut bits = *word;
            std::iter::from_fn(move || {
                let b = bits.trailing_zeros();
                (bits != 0).then(|| {
                    bits &= bits - 1;
                    (w << 6) + to_usize(b)
                })
            })
        })
    }

    fn count(&self) -> usize {
        self.0.iter().map(|x| to_usize(x.count_ones())).sum()
    }
}

/// What a node holds in one child's place.
#[derive(Debug, Clone, Copy)]
enum Slot {
    Child(u32),
    Tile,
    Empty,
}

/// Generates an internal node type: `$dim_log2` is log2 of its edge in children,
/// `$child_log2` log2 of a child's edge in voxels, `$words` the `u64`s of a mask.
macro_rules! internal_node {
    ($(#[$doc:meta])* $name:ident, $dim_log2:expr, $child_log2:expr, $words:expr) => {
        $(#[$doc])*
        #[derive(Debug, Clone)]
        struct $name {
            children: Mask<$words>,
            tiles: Mask<$words>,
            /// Child numbers, in the order of their bits in `children`.
            child_ids: Vec<u32>,
        }

        impl $name {
            fn new(tiles: Mask<$words>) -> Self {
                Self { children: Mask::EMPTY, tiles, child_ids: Vec::new() }
            }

            fn index(v: Coord) -> usize {
                child_index(v, $child_log2, $dim_log2)
            }

            fn slot(&self, v: Coord) -> Slot {
                let i = Self::index(v);
                if self.children.get(i) {
                    Slot::Child(self.child_ids[self.children.count_before(i)])
                } else if self.tiles.get(i) {
                    Slot::Tile
                } else {
                    Slot::Empty
                }
            }

            /// Puts child `id` in the place holding `v`, which was empty or a tile.
            fn insert_child(&mut self, v: Coord, id: u32) {
                let i = Self::index(v);
                debug_assert!(!self.children.get(i), "the place holds no child");
                self.tiles.clear(i);
                self.child_ids.insert(self.children.count_before(i), id);
                self.children.set(i);
            }

            fn set_tile(&mut self, v: Coord) {
                self.tiles.set(Self::index(v));
            }

            /// Makes the place holding `v` a tile in place of its child, giving the child's
            /// number, or nothing where there was no child.
            fn child_to_tile(&mut self, v: Coord) -> Option<u32> {
                let i = Self::index(v);
                if !self.children.get(i) {
                    return None;
                }
                let id = self.child_ids.remove(self.children.count_before(i));
                self.children.clear(i);
                self.tiles.set(i);
                Some(id)
            }

            /// Each child's place and number, in place order.
            fn children(&self) -> impl Iterator<Item = (usize, u32)> + '_ {
                self.children.ones().zip(self.child_ids.iter().copied())
            }

            /// The same node with its children numbered `ids`, in place order.
            fn with_ids(&self, ids: Vec<u32>) -> Self {
                debug_assert_eq!(ids.len(), self.child_ids.len(), "one number per child");
                Self { children: self.children.clone(), tiles: self.tiles.clone(), child_ids: ids }
            }
        }
    };
}

internal_node!(
    /// An internal node of 32³ children, each a lower node or a tile of 128³ voxels.
    Upper, UPPER_DIM_LOG2, LOWER_SPAN_LOG2, 512
);
internal_node!(
    /// An internal node of 16³ children, each a leaf or a tile of 8³ voxels.
    Lower, LOWER_DIM_LOG2, LEAF_LOG2, 64
);

impl Lower {
    /// Whether every place holds a leaf or a tile.
    fn is_full(&self) -> bool {
        self.children.count() + self.tiles.count() == 16 * 16 * 16
    }
}

/// What the root holds for one upper node's region.
#[derive(Debug, Clone, Copy)]
enum RootEntry {
    Node(u32),
    Tile,
}

/// A leaf of a tree, by its number in that tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LeafId(u32);

impl LeafId {
    /// The leaf's number.
    pub fn get(self) -> u32 {
        self.0
    }
}

/// The level a tile sits at, which sets the region it stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TileLevel {
    /// In the root, in an upper node's place: 4096³ voxels.
    Root,
    /// In an upper node, in a lower node's place: 128³ voxels.
    Upper,
    /// In a lower node, in a leaf's place: 8³ voxels.
    Lower,
}

impl TileLevel {
    /// log2 of the edge, in voxels, of the region a tile at this level stands for.
    pub fn edge_log2(self) -> u32 {
        match self {
            TileLevel::Root => UPPER_SPAN_LOG2,
            TileLevel::Upper => LOWER_SPAN_LOG2,
            TileLevel::Lower => LEAF_LOG2,
        }
    }

    /// The origin of the region at this level holding the voxel.
    pub fn origin(self, v: Coord) -> Coord {
        align(v, self.edge_log2())
    }
}

/// What holds a voxel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Probe {
    /// A leaf.
    Leaf(LeafId),
    /// A tile at this level.
    Tile(TileLevel),
    /// Nothing: the voxel is in the background.
    Background,
}

/// What activating a voxel did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activation {
    /// The voxel's leaf was there already.
    Existing(LeafId),
    /// The voxel's leaf was created, with the nodes above it that were missing.
    Created(LeafId),
    /// A tile at this level holds the voxel, which is active already; nothing changed.
    InTile(TileLevel),
}

/// The sparse tree: which regions are leaves, which are tiles, the rest the background.
/// A merge leaves arena entries the root no longer reaches; queries walk from the root, and
/// `compacted` drops them.
#[derive(Debug, Clone, Default)]
pub struct Tree {
    root: BTreeMap<Coord, RootEntry>,
    upper: Vec<Upper>,
    lower: Vec<Lower>,
    /// Each leaf's origin, its smallest voxel.
    leaves: Vec<Coord>,
}

fn push<T>(arena: &mut Vec<T>, item: T, kind: NodeKind) -> Result<u32, TreeError> {
    let id = u32::try_from(arena.len()).map_err(|_| TreeError::ArenaFull(kind))?;
    arena.push(item);
    Ok(id)
}

impl Tree {
    /// An empty tree: every voxel in the background.
    pub fn new() -> Self {
        Self { root: BTreeMap::new(), upper: Vec::new(), lower: Vec::new(), leaves: Vec::new() }
    }

    /// The upper nodes the root reaches, with their origins, by root key.
    fn reached_upper(&self) -> impl Iterator<Item = (Coord, &Upper)> + '_ {
        self.root.iter().filter_map(|(key, entry)| match entry {
            RootEntry::Node(u) => Some((*key, &self.upper[to_usize(*u)])),
            RootEntry::Tile => None,
        })
    }

    /// The lower nodes the root reaches, with their origins, in walking order.
    fn reached_lower(&self) -> impl Iterator<Item = (Coord, &Lower)> + '_ {
        self.reached_upper().flat_map(move |(o, node)| {
            node.children().map(move |(i, l)| {
                (child_origin(o, i, LOWER_SPAN_LOG2, UPPER_DIM_LOG2), &self.lower[to_usize(l)])
            })
        })
    }

    /// The number of leaves.
    pub fn leaf_count(&self) -> usize {
        self.reached_lower().map(|(_, n)| n.children.count()).sum()
    }

    /// The number of tiles at a level.
    pub fn tile_count(&self, level: TileLevel) -> usize {
        match level {
            TileLevel::Root => self.root.values().filter(|e| matches!(e, RootEntry::Tile)).count(),
            TileLevel::Upper => self.reached_upper().map(|(_, n)| n.tiles.count()).sum(),
            TileLevel::Lower => self.reached_lower().map(|(_, n)| n.tiles.count()).sum(),
        }
    }

    /// A leaf's origin, its smallest voxel.
    pub fn leaf_origin(&self, leaf: LeafId) -> Result<Coord, TreeError> {
        self.leaves.get(to_usize(leaf.0)).copied().ok_or(TreeError::UnknownLeaf(leaf.0))
    }

    /// Every leaf with its origin, by root key, then by place in each node.
    pub fn leaves(&self) -> impl Iterator<Item = (LeafId, Coord)> + '_ {
        self.reached_lower().flat_map(move |(o, node)| {
            node.children()
                .map(move |(j, id)| (LeafId(id), child_origin(o, j, LEAF_LOG2, LOWER_DIM_LOG2)))
        })
    }

    /// Every tile with its level and origin, by root key, then by place in each node.
    pub fn tiles(&self) -> Vec<(TileLevel, Coord)> {
        let mut out = Vec::new();
        for (key, entry) in &self.root {
            match entry {
                RootEntry::Tile => out.push((TileLevel::Root, *key)),
                RootEntry::Node(u) => self.upper_tiles(*u, *key, &mut out),
            }
        }
        out
    }

    fn upper_tiles(&self, u: u32, origin: Coord, out: &mut Vec<(TileLevel, Coord)>) {
        let node = &self.upper[to_usize(u)];
        let at = |i| child_origin(origin, i, LOWER_SPAN_LOG2, UPPER_DIM_LOG2);
        out.extend(node.tiles.ones().map(|i| (TileLevel::Upper, at(i))));
        for (i, l) in node.children() {
            let lower = &self.lower[to_usize(l)];
            let o = at(i);
            out.extend(
                lower
                    .tiles
                    .ones()
                    .map(|j| (TileLevel::Lower, child_origin(o, j, LEAF_LOG2, LOWER_DIM_LOG2))),
            );
        }
    }

    /// What holds the voxel.
    pub fn probe(&self, v: Coord) -> Probe {
        let u = match self.root.get(&align(v, UPPER_SPAN_LOG2)) {
            None => return Probe::Background,
            Some(RootEntry::Tile) => return Probe::Tile(TileLevel::Root),
            Some(RootEntry::Node(u)) => *u,
        };
        let l = match self.upper[to_usize(u)].slot(v) {
            Slot::Empty => return Probe::Background,
            Slot::Tile => return Probe::Tile(TileLevel::Upper),
            Slot::Child(l) => l,
        };
        match self.lower[to_usize(l)].slot(v) {
            Slot::Empty => Probe::Background,
            Slot::Tile => Probe::Tile(TileLevel::Lower),
            Slot::Child(id) => Probe::Leaf(LeafId(id)),
        }
    }

    /// Makes the voxel's region a leaf, creating the nodes above it that are missing.
    /// Inside a tile it changes nothing (ADR-0013).
    pub fn activate(&mut self, v: Coord) -> Result<Activation, TreeError> {
        let u = match self.root.get(&align(v, UPPER_SPAN_LOG2)) {
            Some(RootEntry::Tile) => return Ok(Activation::InTile(TileLevel::Root)),
            Some(RootEntry::Node(u)) => *u,
            None => self.new_upper(v, Mask::EMPTY)?,
        };
        let l = match self.upper[to_usize(u)].slot(v) {
            Slot::Tile => return Ok(Activation::InTile(TileLevel::Upper)),
            Slot::Child(l) => l,
            Slot::Empty => self.new_lower(u, v, Mask::EMPTY)?,
        };
        match self.lower[to_usize(l)].slot(v) {
            Slot::Tile => Ok(Activation::InTile(TileLevel::Lower)),
            Slot::Child(id) => Ok(Activation::Existing(LeafId(id))),
            Slot::Empty => self.new_leaf(l, v).map(Activation::Created),
        }
    }

    /// Makes the region of the given level holding the voxel a tile, creating the nodes
    /// above it that are missing. The region must hold nothing and lie in no tile.
    pub fn add_tile(&mut self, level: TileLevel, v: Coord) -> Result<(), TreeError> {
        let occupied = TreeError::Occupied { level, voxel: v };
        let key = align(v, UPPER_SPAN_LOG2);
        let u = match (level, self.root.get(&key)) {
            (TileLevel::Root, None) => {
                self.root.insert(key, RootEntry::Tile);
                return Ok(());
            }
            (TileLevel::Root, Some(_)) | (_, Some(RootEntry::Tile)) => return Err(occupied),
            (TileLevel::Upper | TileLevel::Lower, Some(RootEntry::Node(u))) => *u,
            (TileLevel::Upper | TileLevel::Lower, None) => self.new_upper(v, Mask::EMPTY)?,
        };
        let l = match (level, self.upper[to_usize(u)].slot(v)) {
            (TileLevel::Upper, Slot::Empty) => {
                self.upper[to_usize(u)].set_tile(v);
                return Ok(());
            }
            (TileLevel::Lower, Slot::Child(l)) => l,
            (TileLevel::Lower, Slot::Empty) => self.new_lower(u, v, Mask::EMPTY)?,
            (TileLevel::Root | TileLevel::Upper, _) | (TileLevel::Lower, Slot::Tile) => {
                return Err(occupied);
            }
        };
        match self.lower[to_usize(l)].slot(v) {
            Slot::Empty => {
                self.lower[to_usize(l)].set_tile(v);
                Ok(())
            }
            Slot::Tile | Slot::Child(_) => Err(occupied),
        }
    }

    /// Splits the tile holding the voxel down to the voxel's leaf: each level on the path
    /// becomes a node whose other children are tiles (ADR-0013).
    pub fn split_tile(&mut self, v: Coord) -> Result<LeafId, TreeError> {
        let u = match self.root.get(&align(v, UPPER_SPAN_LOG2)) {
            None => return Err(TreeError::NotATile(v)),
            // `new_upper` replaces the root's tile with a node of tiles.
            Some(RootEntry::Tile) => {
                let u = self.new_upper(v, Mask::FULL)?;
                return self.split_upper_tile(u, v);
            }
            Some(RootEntry::Node(u)) => *u,
        };
        let l = match self.upper[to_usize(u)].slot(v) {
            Slot::Empty => return Err(TreeError::NotATile(v)),
            Slot::Tile => return self.split_upper_tile(u, v),
            Slot::Child(l) => l,
        };
        match self.lower[to_usize(l)].slot(v) {
            Slot::Tile => self.new_leaf(l, v),
            Slot::Empty | Slot::Child(_) => Err(TreeError::NotATile(v)),
        }
    }

    /// Merges the child holding the voxel into a tile at `level`: a leaf into a tile in its
    /// lower node (`Lower`), or a lower node into a tile in its upper node (`Upper`), which
    /// every place of the lower node must fill, with leaves or tiles (ADR-0014). Merging into
    /// a root tile is not built. The merged child's arena entries become unreachable.
    pub fn merge(&mut self, level: TileLevel, v: Coord) -> Result<(), TreeError> {
        if level == TileLevel::Root {
            return Err(TreeError::MergeUnsupported(level));
        }
        let none = TreeError::NotMergeable { level, voxel: v };
        let u = match self.root.get(&align(v, UPPER_SPAN_LOG2)) {
            Some(RootEntry::Node(u)) => *u,
            Some(RootEntry::Tile) | None => return Err(none),
        };
        let l = match self.upper[to_usize(u)].slot(v) {
            Slot::Child(l) => l,
            Slot::Tile | Slot::Empty => return Err(none),
        };
        match level {
            TileLevel::Root => Err(none),
            TileLevel::Upper if !self.lower[to_usize(l)].is_full() => {
                Err(TreeError::HoldsEmpty { level, voxel: v })
            }
            TileLevel::Upper => {
                self.upper[to_usize(u)].child_to_tile(v);
                Ok(())
            }
            TileLevel::Lower => self.lower[to_usize(l)].child_to_tile(v).map(|_| ()).ok_or(none),
        }
    }

    /// The same tree with arenas holding only what the root reaches, numbered in walking
    /// order.
    pub fn compacted(&self) -> Result<Tree, TreeError> {
        let mut out = Tree::new();
        for (key, entry) in &self.root {
            let RootEntry::Node(u) = entry else {
                out.root.insert(*key, RootEntry::Tile);
                continue;
            };
            let mut lowers = Vec::new();
            for (_, l) in self.upper[to_usize(*u)].children() {
                let lower = &self.lower[to_usize(l)];
                let mut ids = Vec::new();
                for (_, leaf) in lower.children() {
                    ids.push(push(&mut out.leaves, self.leaves[to_usize(leaf)], NodeKind::Leaf)?);
                }
                lowers.push(push(&mut out.lower, lower.with_ids(ids), NodeKind::Lower)?);
            }
            let node = self.upper[to_usize(*u)].with_ids(lowers);
            let id = push(&mut out.upper, node, NodeKind::Upper)?;
            out.root.insert(*key, RootEntry::Node(id));
        }
        Ok(out)
    }

    /// The arena lengths: upper nodes, lower nodes, leaves.
    #[cfg(test)]
    pub(crate) fn arena_lens(&self) -> [usize; 3] {
        [self.upper.len(), self.lower.len(), self.leaves.len()]
    }

    /// The place holding `v` in upper node `u` is a tile: it becomes a lower node of tiles,
    /// whose place holding `v` becomes a leaf.
    fn split_upper_tile(&mut self, u: u32, v: Coord) -> Result<LeafId, TreeError> {
        let l = self.new_lower(u, v, Mask::FULL)?;
        self.new_leaf(l, v)
    }

    /// A new upper node for the region holding `v`, put in the root.
    fn new_upper(&mut self, v: Coord, tiles: Mask<512>) -> Result<u32, TreeError> {
        let id = push(&mut self.upper, Upper::new(tiles), NodeKind::Upper)?;
        self.root.insert(align(v, UPPER_SPAN_LOG2), RootEntry::Node(id));
        Ok(id)
    }

    /// A new lower node for the region holding `v`, put in upper node `u`.
    fn new_lower(&mut self, u: u32, v: Coord, tiles: Mask<64>) -> Result<u32, TreeError> {
        let id = push(&mut self.lower, Lower::new(tiles), NodeKind::Lower)?;
        self.upper[to_usize(u)].insert_child(v, id);
        Ok(id)
    }

    /// A new leaf for the region holding `v`, put in lower node `l`.
    fn new_leaf(&mut self, l: u32, v: Coord) -> Result<LeafId, TreeError> {
        let id = push(&mut self.leaves, align(v, LEAF_LOG2), NodeKind::Leaf)?;
        self.lower[to_usize(l)].insert_child(v, id);
        Ok(LeafId(id))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use proptest::prelude::*;

    use super::*;

    #[test]
    fn mask_offsets_cross_word_boundaries() {
        let mut m = Mask::<2>::EMPTY;
        for i in [0, 63, 64, 127] {
            m.set(i);
        }
        let got: Vec<usize> = [0, 1, 63, 64, 65, 127].map(|i| m.count_before(i)).to_vec();
        assert_eq!(got, vec![0, 1, 1, 2, 3, 3]);
        assert_eq!(m.count(), 4);
        m.clear(63);
        assert_eq!(m.count_before(127), 2);
    }

    #[test]
    fn negative_coordinates_round_down() {
        let mut t = Tree::new();
        let Activation::Created(id) = t.activate([-1, -1, -1]).unwrap() else { panic!() };
        assert_eq!(t.leaf_origin(id), Ok([-8, -8, -8]));
        assert_eq!(t.probe([-8, -8, -8]), Probe::Leaf(id));
        assert_eq!(t.probe([-9, -1, -1]), Probe::Background);
        assert_eq!(t.probe([0, 0, 0]), Probe::Background);
        assert_eq!(t.activate([-8, -2, -5]), Ok(Activation::Existing(id)));
    }

    #[test]
    fn leaves_either_side_of_a_root_boundary_differ() {
        let mut t = Tree::new();
        let a = t.activate([4095, 0, 0]).unwrap();
        let b = t.activate([4096, 0, 0]).unwrap();
        assert_ne!(a, b);
        assert_eq!(t.leaf_count(), 2);
        let Probe::Leaf(id) = t.probe([4096, 7, 7]) else { panic!() };
        assert_eq!(t.leaf_origin(id), Ok([4096, 0, 0]));
    }

    #[test]
    fn splitting_a_root_tile_leaves_its_siblings_as_tiles() {
        let mut t = Tree::new();
        t.add_tile(TileLevel::Root, [0, 0, 0]).unwrap();
        let id = t.split_tile([5, 130, 9]).unwrap();
        assert_eq!(t.leaf_origin(id), Ok([0, 128, 8]));
        assert_eq!(t.tile_count(TileLevel::Root), 0);
        assert_eq!(t.tile_count(TileLevel::Upper), 32 * 32 * 32 - 1);
        assert_eq!(t.tile_count(TileLevel::Lower), 16 * 16 * 16 - 1);
        assert_eq!(t.probe([5, 130, 9]), Probe::Leaf(id));
        assert_eq!(t.probe([0, 0, 0]), Probe::Tile(TileLevel::Upper));
        assert_eq!(t.probe([0, 128, 0]), Probe::Tile(TileLevel::Lower));
        assert_eq!(t.probe([4095, 4095, 4095]), Probe::Tile(TileLevel::Upper));
        assert_eq!(t.probe([4096, 0, 0]), Probe::Background);
    }

    #[test]
    fn activation_inside_a_tile_changes_nothing() {
        let mut t = Tree::new();
        t.add_tile(TileLevel::Upper, [200, 0, 0]).unwrap();
        assert_eq!(t.activate([130, 5, 5]), Ok(Activation::InTile(TileLevel::Upper)));
        assert_eq!(t.leaf_count(), 0);
        assert_eq!(t.tile_count(TileLevel::Upper), 1);
    }

    #[test]
    fn a_tile_is_refused_where_anything_is() {
        let mut t = Tree::new();
        t.activate([0, 0, 0]).unwrap();
        for (level, v) in [
            (TileLevel::Lower, [1, 1, 1]),
            (TileLevel::Upper, [100, 0, 0]),
            (TileLevel::Root, [-0, 4000, 0]),
        ] {
            assert_eq!(t.add_tile(level, v), Err(TreeError::Occupied { level, voxel: v }));
        }
        t.add_tile(TileLevel::Lower, [8, 0, 0]).unwrap();
        assert!(t.add_tile(TileLevel::Lower, [9, 0, 0]).is_err());
        t.add_tile(TileLevel::Root, [-1, 0, 0]).unwrap();
        assert!(t.add_tile(TileLevel::Lower, [-5, 0, 0]).is_err());
        assert!(t.add_tile(TileLevel::Upper, [-5, 0, 0]).is_err());
    }

    #[test]
    fn merges_take_leaves_and_tiles_and_refuse_an_empty_place() {
        let mut t = Tree::new();
        t.activate([0, 0, 0]).unwrap();
        t.merge(TileLevel::Lower, [3, 3, 3]).unwrap();
        assert_eq!(t.probe([0, 0, 0]), Probe::Tile(TileLevel::Lower));
        assert_eq!(t.leaf_count(), 0);
        let none = TreeError::NotMergeable { level: TileLevel::Lower, voxel: [0, 0, 0] };
        assert_eq!(t.merge(TileLevel::Lower, [0, 0, 0]), Err(none));
        let empty = TreeError::HoldsEmpty { level: TileLevel::Upper, voxel: [0, 0, 0] };
        assert_eq!(t.merge(TileLevel::Upper, [0, 0, 0]), Err(empty));
        for i in 1..4096 {
            let v = [i % 16 * 8, i / 16 % 16 * 8, i / 256 * 8];
            if i % 2 == 0 { t.activate(v).map(|_| ()) } else { t.add_tile(TileLevel::Lower, v) }
                .unwrap();
        }
        t.merge(TileLevel::Upper, [127, 0, 64]).unwrap();
        assert_eq!(t.probe([5, 120, 9]), Probe::Tile(TileLevel::Upper));
        assert_eq!(
            (t.leaf_count(), t.tile_count(TileLevel::Lower), t.tile_count(TileLevel::Upper)),
            (0, 0, 1)
        );
        assert_eq!(
            t.merge(TileLevel::Root, [0, 0, 0]),
            Err(TreeError::MergeUnsupported(TileLevel::Root))
        );
    }

    #[test]
    fn compaction_drops_what_the_root_no_longer_reaches() {
        let mut t = Tree::new();
        for x in 0..40 {
            t.activate([x * 8, 0, 0]).unwrap();
        }
        // 40 leaves over three lower nodes; the first node's 16 merge into tiles.
        for x in 0..16 {
            t.merge(TileLevel::Lower, [x * 8, 0, 0]).unwrap();
        }
        assert_eq!(t.arena_lens(), [1, 3, 40]);
        let c = t.compacted().unwrap();
        assert_eq!(c.arena_lens(), [1, 3, 24]);
        let walk = |t: &Tree| -> Vec<Coord> { t.leaves().map(|(_, o)| o).collect() };
        assert_eq!(walk(&c), walk(&t));
        assert_eq!(c.tiles(), t.tiles());
        for x in -1..60 {
            let v = [x * 4, 1, 2];
            let kind = |p: Probe| match p {
                Probe::Leaf(_) => None,
                Probe::Tile(l) => Some(Some(l)),
                Probe::Background => Some(None),
            };
            assert_eq!(kind(c.probe(v)), kind(t.probe(v)), "at {v:?}");
        }
    }

    #[test]
    fn only_a_tile_splits() {
        let mut t = Tree::new();
        assert_eq!(t.split_tile([0, 0, 0]), Err(TreeError::NotATile([0, 0, 0])));
        t.activate([0, 0, 0]).unwrap();
        assert_eq!(t.split_tile([0, 0, 0]), Err(TreeError::NotATile([0, 0, 0])));
        assert_eq!(t.split_tile([8, 0, 0]), Err(TreeError::NotATile([8, 0, 0])));
        assert_eq!(t.leaf_origin(LeafId(1)), Err(TreeError::UnknownLeaf(1)));
    }

    // ---- Reference tier: the tree against a model holding regions as plain sets.

    fn span(level: TileLevel) -> u32 {
        level.edge_log2()
    }

    const LEVELS: [TileLevel; 3] = [TileLevel::Root, TileLevel::Upper, TileLevel::Lower];

    /// Leaves by origin and tiles by level and origin, with no nodes and no numbering.
    #[derive(Default)]
    struct Model {
        leaves: BTreeSet<Coord>,
        tiles: BTreeSet<(TileLevel, Coord)>,
    }

    #[derive(Debug, PartialEq)]
    enum Seen {
        Leaf(Coord),
        Tile(TileLevel),
        Background,
    }

    impl Model {
        fn tile_at(&self, v: Coord) -> Option<TileLevel> {
            LEVELS.into_iter().find(|l| self.tiles.contains(&(*l, align(v, span(*l)))))
        }

        fn probe(&self, v: Coord) -> Seen {
            match self.tile_at(v) {
                Some(l) => Seen::Tile(l),
                None if self.leaves.contains(&align(v, LEAF_LOG2)) => {
                    Seen::Leaf(align(v, LEAF_LOG2))
                }
                None => Seen::Background,
            }
        }

        /// The result's kind, and the leaf origin where there is one.
        fn activate(&mut self, v: Coord) -> (u8, Option<Coord>) {
            if self.tile_at(v).is_some() {
                return (2, None);
            }
            let o = align(v, LEAF_LOG2);
            (u8::from(self.leaves.insert(o)), Some(o))
        }

        /// Whether two aligned cubes meet: one holds the other.
        fn meet(a: (u32, Coord), b: (u32, Coord)) -> bool {
            let m = a.0.max(b.0);
            align(a.1, m) == align(b.1, m)
        }

        fn add_tile(&mut self, level: TileLevel, v: Coord) -> bool {
            let r = (span(level), align(v, span(level)));
            // A region meets what holds it (a tile at its level or above) or what it holds
            // (a leaf or smaller tile whose origin lies in it, found by a range on x).
            let x = r.1[0]..r.1[0] + (1 << r.0);
            let inside = |o: &Coord| Self::meet(r, (LEAF_LOG2, *o)) && x.contains(&o[0]);
            let from = |x0: i32| [x0, i32::MIN, i32::MIN];
            let busy = LEVELS.iter().any(|l| {
                (span(*l) >= r.0 && self.tiles.contains(&(*l, align(v, span(*l)))))
                    || (span(*l) < r.0
                        && self
                            .tiles
                            .range((*l, from(x.start))..(*l, from(x.end)))
                            .any(|(_, o)| inside(o)))
            }) || self.leaves.range(from(x.start)..from(x.end)).any(inside);
            if !busy {
                self.tiles.insert((level, r.1));
            }
            !busy
        }

        fn split(&mut self, v: Coord) -> Option<Coord> {
            let top = self.tile_at(v)?;
            self.tiles.remove(&(top, align(v, span(top))));
            for (parent, child) in
                [(TileLevel::Root, TileLevel::Upper), (TileLevel::Upper, TileLevel::Lower)]
            {
                if span(parent) > span(top) {
                    continue;
                }
                let o = align(v, span(parent));
                let n = 1 << (span(parent) - span(child));
                for (x, y, z) in
                    (0..n).flat_map(|x| (0..n).flat_map(move |y| (0..n).map(move |z| (x, y, z))))
                {
                    let c = [
                        o[0] + (x << span(child)),
                        o[1] + (y << span(child)),
                        o[2] + (z << span(child)),
                    ];
                    if c != align(v, span(child)) {
                        self.tiles.insert((child, c));
                    }
                }
            }
            let leaf = align(v, LEAF_LOG2);
            self.leaves.insert(leaf);
            Some(leaf)
        }

        /// The leaf-sized regions of the lower node's region holding `v`.
        fn regions(v: Coord) -> impl Iterator<Item = Coord> {
            let o = align(v, LOWER_SPAN_LOG2);
            (0..4096).map(move |i| [o[0] + i % 16 * 8, o[1] + i / 16 % 16 * 8, o[2] + i / 256 * 8])
        }

        fn merge(&mut self, level: TileLevel, v: Coord) -> bool {
            match level {
                TileLevel::Root => false,
                TileLevel::Lower => {
                    let o = align(v, LEAF_LOG2);
                    let ok = self.tile_at(v).is_none() && self.leaves.remove(&o);
                    if ok {
                        self.tiles.insert((TileLevel::Lower, o));
                    }
                    ok
                }
                TileLevel::Upper => {
                    let covered =
                        matches!(self.tile_at(v), Some(TileLevel::Root | TileLevel::Upper));
                    let full = Self::regions(v).all(|r| {
                        self.leaves.contains(&r) || self.tiles.contains(&(TileLevel::Lower, r))
                    });
                    if covered || !full {
                        return false;
                    }
                    for r in Self::regions(v) {
                        self.leaves.remove(&r);
                        self.tiles.remove(&(TileLevel::Lower, r));
                    }
                    self.tiles.insert((TileLevel::Upper, align(v, LOWER_SPAN_LOG2)));
                    true
                }
            }
        }
    }

    #[derive(Debug, Clone)]
    enum Op {
        Activate(Coord),
        AddTile(TileLevel, Coord),
        Split(Coord),
        Merge(TileLevel, Coord),
        /// Every leaf-sized region of a lower node's region made a leaf, or a tile.
        Fill(Coord, bool),
        /// The tree replaced by its compacted copy.
        Compact,
    }

    /// Coordinates near the boundaries of every level, negative ones included, so that
    /// edits meet each other and cross node and root boundaries.
    fn coord() -> impl Strategy<Value = Coord> {
        let anchors = [-4097, -4096, -129, -128, -9, -8, -1, 0, 7, 8, 127, 128, 4095, 4096];
        let axis = (prop::sample::select(anchors.to_vec()), 0..3_i32).prop_map(|(a, d)| a + d);
        [axis.clone(), axis.clone(), axis]
    }

    /// A tile level; Root tiles are rarer, as splitting one makes 36 862 tiles, which the model
    /// holds one by one.
    fn level() -> impl Strategy<Value = TileLevel> {
        prop_oneof![1 => Just(TileLevel::Root), 3 => Just(TileLevel::Upper), 3 => Just(TileLevel::Lower)]
    }

    /// Edits on a few shared coordinates, each moved by a voxel or two, so that most
    /// splits find a tile and most activations meet a leaf or a tile.
    fn ops() -> impl Strategy<Value = Vec<Op>> {
        #[derive(Debug, Clone, Copy)]
        enum Kind {
            Activate,
            AddTile(TileLevel),
            Split,
            Merge(TileLevel),
            Fill(bool),
            Compact,
        }
        let kind = prop_oneof![
            6 => Just(Kind::Activate),
            4 => level().prop_map(Kind::AddTile),
            4 => Just(Kind::Split),
            2 => Just(Kind::Merge(TileLevel::Lower)),
            2 => Just(Kind::Merge(TileLevel::Upper)),
            1 => Just(Kind::Merge(TileLevel::Root)),
            1 => any::<bool>().prop_map(Kind::Fill),
            1 => Just(Kind::Compact),
        ];
        let edit = (kind, 0..6_usize, [0..2_i32, 0..2, 0..2]);
        (prop::collection::vec(coord(), 6), prop::collection::vec(edit, 1..40)).prop_map(
            |(pool, edits)| {
                edits
                    .into_iter()
                    .map(|(kind, i, d)| {
                        let v = [0, 1, 2].map(|k| pool[i][k] + d[k]);
                        match kind {
                            Kind::Activate => Op::Activate(v),
                            Kind::AddTile(l) => Op::AddTile(l, v),
                            Kind::Split => Op::Split(v),
                            Kind::Merge(l) => Op::Merge(l, v),
                            Kind::Fill(leaves) => Op::Fill(v, leaves),
                            Kind::Compact => Op::Compact,
                        }
                    })
                    .collect()
            },
        )
    }

    fn seen(t: &Tree, v: Coord) -> Seen {
        match t.probe(v) {
            Probe::Leaf(id) => Seen::Leaf(t.leaf_origin(id).unwrap()),
            Probe::Tile(l) => Seen::Tile(l),
            Probe::Background => Seen::Background,
        }
    }

    /// The voxels to compare after each edit: the edit's voxel and its neighbours one
    /// voxel, one leaf, one lower node and one upper node away along each axis.
    fn probes(v: Coord) -> Vec<Coord> {
        let mut out = vec![v];
        for axis in 0..3 {
            for d in [-4096, -128, -8, -1, 1, 8, 128, 4096] {
                let mut p = v;
                p[axis] += d;
                out.push(p);
            }
        }
        out
    }

    fn step(t: &mut Tree, m: &mut Model, op: &Op) -> Result<Coord, TestCaseError> {
        let v = match *op {
            Op::Activate(v) => {
                let got = match t.activate(v).unwrap() {
                    Activation::Created(id) => (1, Some(t.leaf_origin(id).unwrap())),
                    Activation::Existing(id) => (0, Some(t.leaf_origin(id).unwrap())),
                    Activation::InTile(_) => (2, None),
                };
                prop_assert_eq!(got, m.activate(v));
                v
            }
            Op::AddTile(l, v) => {
                prop_assert_eq!(t.add_tile(l, v).is_ok(), m.add_tile(l, v));
                v
            }
            Op::Split(v) => {
                let got = t.split_tile(v).ok().map(|id| t.leaf_origin(id).unwrap());
                prop_assert_eq!(got, m.split(v));
                v
            }
            Op::Merge(l, v) => {
                prop_assert_eq!(t.merge(l, v).is_ok(), m.merge(l, v), "merge {:?} at {:?}", l, v);
                v
            }
            Op::Fill(v, leaves) => {
                for r in Model::regions(v) {
                    if leaves {
                        let got = matches!(t.activate(r).unwrap(), Activation::InTile(_));
                        prop_assert_eq!(got, m.activate(r).0 == 2);
                    } else {
                        prop_assert_eq!(
                            t.add_tile(TileLevel::Lower, r).is_ok(),
                            m.add_tile(TileLevel::Lower, r)
                        );
                    }
                }
                v
            }
            Op::Compact => {
                *t = t.compacted().unwrap();
                [0, 0, 0]
            }
        };
        Ok(v)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        #[test]
        fn tree_matches_the_model(ops in ops()) {
            let mut t = Tree::new();
            let mut m = Model::default();
            for op in &ops {
                let v = step(&mut t, &mut m, op)?;
                for p in probes(v) {
                    prop_assert_eq!(seen(&t, p), m.probe(p), "at {:?} after {:?}", p, op);
                }
                prop_assert_eq!(t.leaf_count(), m.leaves.len());
            }
            for l in LEVELS {
                prop_assert_eq!(t.tile_count(l), m.tiles.iter().filter(|(x, _)| *x == l).count());
            }
            // Once per case: the iterators list exactly the model's leaves and tiles.
            let leaves: BTreeSet<Coord> = t.leaves().map(|(_, o)| o).collect();
            prop_assert_eq!(&leaves, &m.leaves);
            let tiles: BTreeSet<(TileLevel, Coord)> = t.tiles().into_iter().collect();
            prop_assert_eq!(&tiles, &m.tiles);
            // A compacted copy holds the same, with no unreachable arena entry.
            let c = t.compacted().unwrap();
            let lens = c.arena_lens();
            prop_assert_eq!(lens[2], m.leaves.len());
            prop_assert_eq!(c.tiles(), t.tiles());
        }
    }
}
