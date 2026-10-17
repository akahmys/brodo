//! A topology: the tree, a stamp unique in the process, and its leaves and tiles numbered in
//! the Morton order of their origins (ARCHITECTURE.md §4.2, ADR-0014). It is edited only
//! through a `TopologyEditor`, whose commit gives a new topology and the `Remap` to it.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::TreeError;
use crate::manifest::{DIRECTIONS, LeafManifest};
use crate::tree::{Activation, Coord, LeafId, Probe, Step, TileLevel, Tree};

static NEXT_STAMP: AtomicU64 = AtomicU64::new(1);

fn next_stamp() -> u64 {
    NEXT_STAMP.fetch_add(1, Ordering::Relaxed)
}

/// Each byte with its bits spread to every third bit.
const SPREAD: [u32; 256] = {
    let mut t = [0_u32; 256];
    let mut i: u32 = 0;
    while i < 256 {
        let mut b = 0;
        while b < 8 {
            t[i as usize] |= ((i >> b) & 1) << (3 * b);
            b += 1;
        }
        i += 1;
    }
    t
};

/// The Morton key of a coordinate: the bits of the three axes interleaved, x lowest, each
/// axis offset so that negative coordinates come first.
pub(crate) fn morton(c: Coord) -> u128 {
    let spread = |a: i32| {
        let x = a.cast_unsigned() ^ 0x8000_0000;
        x.to_le_bytes().iter().zip([0, 24, 48, 72]).fold(0_u128, |acc, (byte, shift)| {
            acc | (u128::from(SPREAD[usize::from(*byte)]) << shift)
        })
    };
    spread(c[0]) | (spread(c[1]) << 1) | (spread(c[2]) << 2)
}

fn to_usize(i: u32) -> usize {
    // Lossless: brodo's targets have pointers of at least 32 bits (`TARGETS`).
    i as usize
}

/// A leaf of a topology, numbered in the Morton order of the leaves' origins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LeafNumber(pub(crate) u32);

impl LeafNumber {
    /// The number.
    pub fn get(self) -> u32 {
        self.0
    }
}

/// A tile of a topology, numbered in the Morton order of the tiles' origins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TileNumber(pub(crate) u32);

impl TileNumber {
    /// The number.
    pub fn get(self) -> u32 {
        self.0
    }
}

/// What holds a voxel in a topology.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    /// A leaf.
    Leaf(LeafNumber),
    /// A tile, with its level.
    Tile(TileNumber, TileLevel),
    /// Nothing: the background.
    Background,
}

/// The tree with its stamp and numbering.
#[derive(Debug, Clone)]
pub struct Topology {
    tree: Tree,
    stamp: u64,
    /// Leaf origins, by leaf number.
    leaves: Vec<Coord>,
    /// Leaf numbers, by the tree's leaf id.
    leaf_numbers: Vec<u32>,
    /// Tile levels and origins, by tile number.
    tiles: Vec<(TileLevel, Coord)>,
    /// Tile numbers, by origin; tiles never share one.
    tile_numbers: BTreeMap<Coord, u32>,
    manifest: LeafManifest,
}

impl Default for Topology {
    fn default() -> Self {
        Self::new()
    }
}

impl Topology {
    /// An empty topology.
    pub fn new() -> Self {
        Self {
            tree: Tree::new(),
            stamp: next_stamp(),
            leaves: Vec::new(),
            leaf_numbers: Vec::new(),
            tiles: Vec::new(),
            tile_numbers: BTreeMap::new(),
            manifest: LeafManifest::default(),
        }
    }

    fn from_tree(tree: Tree) -> Result<Self, TreeError> {
        let mut leaves: Vec<(LeafId, Coord)> = tree.leaves().collect();
        leaves.sort_by_cached_key(|(_, o)| morton(*o));
        let mut leaf_numbers = vec![0; leaves.len()];
        for ((id, _), n) in leaves.iter().zip(0_u32..) {
            leaf_numbers[to_usize(id.get())] = n;
        }
        let mut tiles = tree.tiles();
        u32::try_from(tiles.len()).map_err(|_| TreeError::TooManyTiles)?;
        tiles.sort_by_cached_key(|(_, o)| morton(*o));
        let tile_numbers = tiles.iter().zip(0_u32..).map(|((_, o), n)| (*o, n)).collect();
        let mut t = Self {
            tree,
            stamp: next_stamp(),
            leaves: leaves.into_iter().map(|(_, o)| o).collect(),
            leaf_numbers,
            tiles,
            tile_numbers,
            manifest: LeafManifest::default(),
        };
        t.manifest = LeafManifest::build(&t.leaves, |v| t.locate(v));
        Ok(t)
    }

    pub(crate) fn stamp(&self) -> u64 {
        self.stamp
    }

    /// The number of leaves.
    pub fn leaf_count(&self) -> usize {
        self.leaves.len()
    }

    /// The number of tiles.
    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    /// Every leaf number, in order.
    pub fn leaf_numbers(&self) -> impl Iterator<Item = LeafNumber> + use<> {
        // The count fits a u32: leaves are numbered by the tree in u32.
        (0..).take(self.leaves.len()).map(LeafNumber)
    }

    /// Every tile number, in order.
    pub fn tile_numbers(&self) -> impl Iterator<Item = TileNumber> + use<> {
        // The count fits a u32: `from_tree` refuses more tiles.
        (0..).take(self.tiles.len()).map(TileNumber)
    }

    /// A leaf's origin, its smallest voxel.
    pub fn leaf_origin(&self, n: LeafNumber) -> Result<Coord, TreeError> {
        self.leaves.get(to_usize(n.0)).copied().ok_or(TreeError::UnknownLeaf(n.0))
    }

    /// A tile's level and origin.
    pub fn tile(&self, n: TileNumber) -> Result<(TileLevel, Coord), TreeError> {
        self.tiles.get(to_usize(n.0)).copied().ok_or(TreeError::UnknownTile(n.0))
    }

    /// The leaf manifest: what lies next to each leaf (§4.3).
    pub fn manifest(&self) -> &LeafManifest {
        &self.manifest
    }

    /// What lies next to a leaf, by direction number (`Offset`).
    pub fn neighbours(&self, n: LeafNumber) -> Result<&[Location; DIRECTIONS], TreeError> {
        self.manifest.neighbours(n)
    }

    /// What holds the voxel.
    pub fn locate(&self, v: Coord) -> Location {
        self.number(self.tree.probe(v), v)
    }

    /// A probe's answer for the voxel, numbered.
    fn number(&self, p: Probe, v: Coord) -> Location {
        match p {
            // Every leaf and tile of the tree was numbered by `from_tree`.
            Probe::Leaf(id) => Location::Leaf(LeafNumber(self.leaf_numbers[to_usize(id.get())])),
            Probe::Tile(level) => {
                Location::Tile(TileNumber(self.tile_numbers[&level.origin(v)]), level)
            }
            Probe::Background => Location::Background,
        }
    }

    /// What holds the voxel, starting from the deepest region of `path` that holds it, and
    /// updating `path` (`ValueAccessor`). `path` must have been used with this topology
    /// only, as an accessor ensures by borrowing it.
    pub(crate) fn locate_on(&self, path: &mut Path, v: Coord) -> Location {
        let (leaf_o, lower_o, upper_o) =
            (TileLevel::Lower.origin(v), TileLevel::Upper.origin(v), TileLevel::Root.origin(v));
        if let Cached::Warm(o, loc) = path.leaf
            && o == leaf_o
        {
            path.hit(0);
            return loc;
        }
        let probe = match (path.lower, path.upper) {
            (Cached::Warm(o, l), _) if o == lower_o => {
                path.hit(1);
                self.tree.lower_step(l, v)
            }
            (_, Cached::Warm(o, u)) if o == upper_o => {
                path.hit(2);
                self.below_upper(path, u, v)
            }
            (Cached::Warm(..) | Cached::Cold, Cached::Warm(..) | Cached::Cold) => {
                path.hit(3);
                match self.tree.root_step(v) {
                    Step::Node(u) => {
                        path.upper = Cached::Warm(upper_o, u);
                        self.below_upper(path, u, v)
                    }
                    Step::Done(p) => p,
                }
            }
        };
        let loc = self.number(probe, v);
        path.leaf = Cached::Warm(leaf_o, loc);
        loc
    }

    fn below_upper(&self, path: &mut Path, u: u32, v: Coord) -> Probe {
        match self.tree.upper_step(u, v) {
            Step::Node(l) => {
                path.lower = Cached::Warm(TileLevel::Upper.origin(v), l);
                self.tree.lower_step(l, v)
            }
            Step::Done(p) => p,
        }
    }

    /// An editor of a copy of this topology, which stays usable with its fields.
    pub fn edit(&self) -> TopologyEditor<'_> {
        TopologyEditor { old: self, tree: self.tree.clone() }
    }
}

/// What a path remembers of one level: nothing yet, or the region it last held, by origin.
#[derive(Debug, Clone, Copy, Default)]
enum Cached<T> {
    #[default]
    Cold,
    Warm(Coord, T),
}

/// The regions last visited in a topology's tree, with their nodes and what held the last
/// leaf-sized region (ARCHITECTURE.md §4.4).
#[derive(Debug, Clone, Default)]
pub(crate) struct Path {
    upper: Cached<u32>,
    lower: Cached<u32>,
    leaf: Cached<Location>,
    /// Lookups answered from the leaf, lower, upper level, and from the root.
    #[cfg(test)]
    pub(crate) hits: [u64; 4],
}

impl Path {
    #[cfg(test)]
    fn hit(&mut self, level: usize) {
        self.hits[level] += 1;
    }

    #[cfg(not(test))]
    fn hit(&mut self, _level: usize) {}
}

/// Edits a copy of a topology; `commit` makes it a new topology.
#[derive(Debug)]
pub struct TopologyEditor<'a> {
    old: &'a Topology,
    tree: Tree,
}

impl TopologyEditor<'_> {
    /// Makes the voxel's region a leaf; inside a tile it changes nothing (`Tree::activate`).
    pub fn activate(&mut self, v: Coord) -> Result<Activation, TreeError> {
        self.tree.activate(v)
    }

    /// Makes the region of the given level holding the voxel a tile (`Tree::add_tile`).
    pub fn add_tile(&mut self, level: TileLevel, v: Coord) -> Result<(), TreeError> {
        self.tree.add_tile(level, v)
    }

    /// Splits the tile holding the voxel down to its leaf (`Tree::split_tile`).
    pub fn split_tile(&mut self, v: Coord) -> Result<(), TreeError> {
        self.tree.split_tile(v).map(|_| ())
    }

    /// Merges the child holding the voxel into a tile at `level` (`Tree::merge`).
    pub fn merge(&mut self, level: TileLevel, v: Coord) -> Result<(), TreeError> {
        self.tree.merge(level, v)
    }

    /// The edited topology, with a new stamp, and the remap to it from the old one. Where
    /// each new value comes from is found by locating each new leaf's and tile's origin in
    /// the old topology, and each region a merged tile covers. The tree's arenas are
    /// rebuilt, so nothing a merge left unreachable outlives the commit.
    pub fn commit(self) -> Result<(Topology, Remap), TreeError> {
        let new = Topology::from_tree(self.tree.compacted()?)?;
        let leaves = new
            .leaves
            .iter()
            .map(|o| match self.old.locate(*o) {
                Location::Leaf(n) => LeafSource::Leaf(n),
                Location::Tile(t, _) => LeafSource::Split(t),
                Location::Background => LeafSource::Background,
            })
            .collect();
        let mut parts = Vec::new();
        let tiles = new
            .tiles
            .iter()
            .map(|(l, o)| tile_source(self.old, *l, *o, &mut parts))
            .collect::<Result<_, _>>()?;
        let remap = Remap { from: self.old.stamp, to: new.stamp, leaves, tiles, parts };
        Ok((new, remap))
    }
}

/// Leaf-sized regions in a lower node's region.
const LOWER_REGIONS: u32 = 4096;

/// Where a new tile at `level` and `origin` comes from, pushing a merged tile's parts.
fn tile_source(
    old: &Topology,
    level: TileLevel,
    origin: Coord,
    parts: &mut Vec<Part>,
) -> Result<TileSource, TreeError> {
    let start = parts.len();
    let was = old.locate(origin);
    match (level, was) {
        // The same tile, or a larger one it was split from.
        (_, Location::Tile(t, l)) if l.edge_log2() >= level.edge_log2() => {
            return Ok(TileSource::Tile(t));
        }
        (TileLevel::Lower, Location::Leaf(n)) => parts.push(Part::Leaf(n)),
        // No lower node there, and no tile (above): the whole region was the background.
        (TileLevel::Upper, _) if !old.tree.has_lower_node(origin) => {
            return Ok(TileSource::Background);
        }
        (TileLevel::Upper, _) => upper_parts(old, origin, parts),
        (TileLevel::Lower | TileLevel::Root, Location::Background) => {
            return Ok(TileSource::Background);
        }
        (TileLevel::Lower | TileLevel::Root, Location::Tile(..))
        | (TileLevel::Root, Location::Leaf(_)) => {
            // A lower tile has no smaller tile, and nothing merges into a root tile.
            debug_assert!(
                matches!(was, Location::Background),
                "no edit makes {level:?} from {was:?}"
            );
            return Ok(TileSource::Background);
        }
    }
    // Only regions that were the background: an added tile, not a merge.
    if parts.get(start..).unwrap_or(&[]).iter().all(|p| matches!(p, Part::Background(_))) {
        parts.truncate(start);
        return Ok(TileSource::Background);
    }
    let at = |i: usize| u32::try_from(i).map_err(|_| TreeError::TooManyParts);
    Ok(TileSource::Merged(PartRange { start: at(start)?, end: at(parts.len())? }))
}

/// The parts of a new upper-level tile: what held each leaf-sized region of its lower
/// node's region, the empty ones counted together.
fn upper_parts(old: &Topology, origin: Coord, parts: &mut Vec<Part>) {
    let mut empty = 0;
    for i in 0..LOWER_REGIONS {
        let at = [i % 16, i / 16 % 16, i / 256].map(|c| c.cast_signed() * 8);
        match old.locate([origin[0] + at[0], origin[1] + at[1], origin[2] + at[2]]) {
            Location::Leaf(n) => parts.push(Part::Leaf(n)),
            Location::Tile(t, _) => parts.push(Part::Tile(t)),
            Location::Background => empty += 1,
        }
    }
    if empty > 0 {
        parts.push(Part::Background(empty));
    }
}

/// Where a new leaf's values come from (ADR-0014).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeafSource {
    /// An old leaf.
    Leaf(LeafNumber),
    /// The old tile it was split from: every voxel takes the tile's value.
    Split(TileNumber),
    /// The background: it was activated where nothing was.
    Background,
}

/// Where a new tile's value comes from (ADR-0014).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileSource {
    /// An old tile: the same one, or a larger one it was split from.
    Tile(TileNumber),
    /// The parts it merges, a range of `Remap::parts`.
    Merged(PartRange),
    /// The background: it was added where nothing was.
    Background,
}

/// A range of a remap's parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartRange {
    start: u32,
    end: u32,
}

/// One source of a merged tile (ADR-0014).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// An old leaf: 512 voxels.
    Leaf(LeafNumber),
    /// An old leaf-sized tile: 512 voxels of one value.
    Tile(TileNumber),
    /// This many leaf-sized regions that were the background: activated in the same commit
    /// as the merge.
    Background(u32),
}

/// From one topology's leaves and tiles to the next one's, made by a commit.
#[derive(Debug, Clone)]
pub struct Remap {
    pub(crate) from: u64,
    pub(crate) to: u64,
    leaves: Vec<LeafSource>,
    tiles: Vec<TileSource>,
    parts: Vec<Part>,
}

impl Remap {
    /// The parts of a merged tile's source; none for any other source or another remap's
    /// range.
    pub fn parts(&self, r: PartRange) -> &[Part] {
        self.parts.get(r.start as usize..r.end as usize).unwrap_or(&[])
    }

    /// Each new leaf's source, by new leaf number.
    pub fn leaf_sources(&self) -> &[LeafSource] {
        &self.leaves
    }

    /// Each new tile's source, by new tile number.
    pub fn tile_sources(&self) -> &[TileSource] {
        &self.tiles
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit_with(t: &Topology, f: impl FnOnce(&mut TopologyEditor<'_>)) -> (Topology, Remap) {
        let mut e = t.edit();
        f(&mut e);
        e.commit().unwrap()
    }

    #[test]
    fn leaves_are_numbered_in_morton_order() {
        let (t, _) = commit_with(&Topology::new(), |e| {
            for v in [[0, 0, 8], [0, 8, 0], [-8, 0, 0], [8, 0, 0], [0, 0, 0]] {
                e.activate(v).unwrap();
            }
        });
        let order: Vec<Coord> = t.leaf_numbers().map(|n| t.leaf_origin(n).unwrap()).collect();
        assert_eq!(order, vec![[-8, 0, 0], [0, 0, 0], [8, 0, 0], [0, 8, 0], [0, 0, 8]]);
        for n in t.leaf_numbers() {
            assert_eq!(t.locate(t.leaf_origin(n).unwrap()), Location::Leaf(n));
        }
    }

    #[test]
    fn tiles_are_numbered_in_morton_order_across_levels() {
        let (t, _) = commit_with(&Topology::new(), |e| {
            e.add_tile(TileLevel::Upper, [128, 0, 0]).unwrap();
            e.add_tile(TileLevel::Root, [-1, 0, 0]).unwrap();
            e.add_tile(TileLevel::Lower, [8, 0, 0]).unwrap();
        });
        let tiles: Vec<(TileLevel, Coord)> = t.tile_numbers().map(|n| t.tile(n).unwrap()).collect();
        assert_eq!(
            tiles,
            vec![
                (TileLevel::Root, [-4096, 0, 0]),
                (TileLevel::Lower, [8, 0, 0]),
                (TileLevel::Upper, [128, 0, 0]),
            ]
        );
        assert_eq!(t.locate([130, 5, 5]), Location::Tile(TileNumber(2), TileLevel::Upper));
    }

    #[test]
    fn a_commit_leaves_the_old_topology_as_it_was() {
        let (t1, _) = commit_with(&Topology::new(), |e| {
            e.activate([0, 0, 0]).unwrap();
        });
        let (t2, r) = commit_with(&t1, |e| {
            e.activate([-1, 0, 0]).unwrap();
            e.add_tile(TileLevel::Lower, [16, 0, 0]).unwrap();
        });
        assert_eq!(t1.leaf_count(), 1);
        assert_eq!(t1.locate([-1, 0, 0]), Location::Background);
        assert_eq!(t2.leaf_count(), 2);
        assert_ne!(t1.stamp(), t2.stamp());
        assert_eq!(r.leaf_sources(), &[LeafSource::Background, LeafSource::Leaf(LeafNumber(0))]);
        assert_eq!(r.tile_sources(), &[TileSource::Background]);
    }

    fn neighbour(t: &Topology, at: Coord, d: [i32; 3]) -> Location {
        let Location::Leaf(n) = t.locate(at) else { panic!("no leaf at {at:?}") };
        let o = crate::manifest::Offset::all().find(|o| o.delta() == d).unwrap();
        t.manifest().neighbour(n, o).unwrap()
    }

    #[test]
    fn neighbours_cross_root_boundaries_and_meet_tiles() {
        let (t, _) = commit_with(&Topology::new(), |e| {
            for v in [[4095, 0, 0], [4096, 0, 0], [-1, -1, -1], [0, 0, 0], [120, 0, 0]] {
                e.activate(v).unwrap();
            }
            e.add_tile(TileLevel::Upper, [128, 0, 0]).unwrap();
        });
        let leaf_at = |v| t.locate(v);
        assert_eq!(neighbour(&t, [4095, 0, 0], [1, 0, 0]), leaf_at([4096, 0, 0]));
        assert_eq!(neighbour(&t, [4096, 0, 0], [-1, 0, 0]), leaf_at([4095, 0, 0]));
        assert_eq!(neighbour(&t, [0, 0, 0], [-1, -1, -1]), leaf_at([-1, -1, -1]));
        assert_eq!(neighbour(&t, [-1, -1, -1], [1, 1, 1]), leaf_at([0, 0, 0]));
        assert_eq!(neighbour(&t, [0, 0, 0], [0, 1, 0]), Location::Background);
        assert!(matches!(
            neighbour(&t, [120, 0, 0], [1, 0, 0]),
            Location::Tile(_, TileLevel::Upper)
        ));
    }

    #[test]
    fn neighbours_beyond_the_coordinate_range_are_the_background() {
        let (t, _) = commit_with(&Topology::new(), |e| {
            e.activate([i32::MAX, i32::MIN, 0]).unwrap();
            e.activate([i32::MAX - 8, i32::MIN, 0]).unwrap();
        });
        let at = [i32::MAX, i32::MIN, 0];
        assert_eq!(neighbour(&t, at, [1, 0, 0]), Location::Background);
        assert_eq!(neighbour(&t, at, [0, -1, 0]), Location::Background);
        assert_eq!(neighbour(&t, at, [-1, 0, 0]), t.locate([i32::MAX - 8, i32::MIN, 0]));
    }

    #[test]
    fn a_commit_keeps_no_arena_entry_a_merge_left() {
        let (t1, _) = commit_with(&Topology::new(), |e| {
            for x in 0..32 {
                e.activate([x * 8, 0, 0]).unwrap();
            }
        });
        let (t2, _) = commit_with(&t1, |e| {
            for x in 0..16 {
                e.merge(TileLevel::Lower, [x * 8, 0, 0]).unwrap();
            }
        });
        assert_eq!(t1.tree.arena_lens(), [1, 2, 32]);
        assert_eq!(t2.tree.arena_lens(), [1, 2, 16]);
        assert_eq!((t2.leaf_count(), t2.tile_count()), (16, 16));
    }

    #[test]
    fn a_split_records_the_tile_each_new_part_came_from() {
        let (t1, _) = commit_with(&Topology::new(), |e| {
            e.add_tile(TileLevel::Upper, [0, 0, 0]).unwrap();
        });
        let (t2, r) = commit_with(&t1, |e| e.split_tile([9, 0, 0]).unwrap());
        assert_eq!(t2.leaf_count(), 1);
        assert_eq!(t2.tile_count(), 4095);
        assert_eq!(r.leaf_sources(), &[LeafSource::Split(TileNumber(0))]);
        assert!(r.tile_sources().iter().all(|s| *s == TileSource::Tile(TileNumber(0))));
    }
}
