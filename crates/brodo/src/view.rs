//! Reading a frozen grid in place (ARCHITECTURE.md §4.6, ADR-0016). This module parses
//! untrusted bytes: it is listed in `UNTRUSTED_INPUT`, indexes nothing (CODING.md Rule 3),
//! checks the whole buffer before any read, and is fuzzed (TESTING.md T6).

use zerocopy::little_endian::U32;
use zerocopy::{FromBytes, IntoBytes};

use crate::error::FormatError;
use crate::field::{Field, LEAF_VOXELS, Stored};
use crate::frozen::{
    BLOCK, Header, LOWER_PLACES, LowerNode, MAGIC, ROOT_NODE, ROOT_TILE, RootEntry, UPPER_PLACES,
    UpperNode, VERSION, local, place,
};
use crate::topology::{Location, Topology};
use crate::tree::{Coord, TileLevel};

/// What holds a voxel in a view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewLocation {
    /// A leaf, by number: the topology's leaf number.
    Leaf(u32),
    /// A tile, by its place among the view's tile values, with its level.
    Tile(u32, TileLevel),
    /// Nothing: the background.
    Background,
}

/// A frozen grid read in place: no copy, every link checked when it was made.
#[derive(Debug, Clone, Copy)]
pub struct GridView<'a, T> {
    roots: &'a [RootEntry],
    uppers: &'a [UpperNode],
    lowers: &'a [LowerNode],
    leaves: &'a [[T; LEAF_VOXELS]],
    tiles: &'a [T],
    background: T,
}

const NAMES: [&str; 5] = ["root", "upper node", "lower node", "leaf value", "tile value"];

fn lost(what: &'static str) -> FormatError {
    FormatError::Inconsistent(what)
}

/// Bits set below `p` in a mask.
fn count_before(mask: &[U32], p: usize) -> u32 {
    let whole: u32 = mask.iter().take(p / 32).map(|w| w.get().count_ones()).sum();
    let part = mask.get(p / 32).map_or(0, |w| (w.get() & ((1_u32 << (p % 32)) - 1)).count_ones());
    whole + part
}

fn is_set(mask: &[U32], p: usize) -> bool {
    mask.get(p / 32).is_some_and(|w| (w.get() >> (p % 32)) & 1 == 1)
}

/// `first` plus the set bits below `p`: the number of the child or tile at `p`.
fn nth(first: U32, mask: &[U32], p: usize, what: &'static str) -> Result<u32, FormatError> {
    first.get().checked_add(count_before(mask, p)).ok_or(lost(what))
}

fn ones(mask: &[U32]) -> u32 {
    mask.iter().map(|w| w.get().count_ones()).sum()
}

/// Whether `first + set bits of mask` fits within `len`, and the masks share no bit.
fn links_fit(children: &[U32], tiles: &[U32], first: [U32; 2], len: [usize; 2]) -> bool {
    let fits = |f: U32, mask: &[U32], n: usize| {
        f.get()
            .checked_add(ones(mask))
            .and_then(|end| usize::try_from(end).ok())
            .is_some_and(|end| end <= n)
    };
    let [fc, ft] = first;
    let [nc, nt] = len;
    fits(fc, children, nc)
        && fits(ft, tiles, nt)
        && children.iter().zip(tiles).all(|(c, t)| c.get() & t.get() == 0)
}

fn check_header<T: Stored>(h: &Header, len: usize) -> Result<(), FormatError> {
    if h.magic.get() != MAGIC {
        return Err(FormatError::BadMagic(h.magic.get()));
    }
    if h.version.get() != VERSION {
        return Err(FormatError::UnsupportedVersion(h.version.get()));
    }
    let size_ok = usize::try_from(h.value_size.get()).is_ok_and(|s| s == size_of::<T>());
    if h.value_type.get() != T::TYPE_CODE || !size_ok {
        return Err(FormatError::WrongValueType {
            expected: T::TYPE_CODE,
            found: h.value_type.get(),
        });
    }
    let blocks = usize::try_from(h.blocks.get()).ok().and_then(|b| b.checked_mul(BLOCK));
    if blocks != Some(len) {
        return Err(lost("header"));
    }
    Ok(())
}

/// The sections' bytes, each where the header says, in order, none overlapping.
fn sections<'a, T>(h: &Header, bytes: &'a [u8]) -> Result<[&'a [u8]; 5], FormatError> {
    let sizes = [
        size_of::<RootEntry>(),
        size_of::<UpperNode>(),
        size_of::<LowerNode>(),
        size_of::<[T; LEAF_VOXELS]>(),
        size_of::<T>(),
    ];
    let mut at = size_of::<Header>();
    let mut out: [&[u8]; 5] = [&[]; 5];
    for ((((slot, size), count), offset), name) in
        out.iter_mut().zip(sizes).zip(h.counts).zip(h.offsets).zip(NAMES)
    {
        let start = usize::try_from(offset.get())
            .ok()
            .and_then(|o| o.checked_mul(BLOCK))
            .ok_or(lost(name))?;
        let len = usize::try_from(count.get())
            .ok()
            .and_then(|c| c.checked_mul(size))
            .ok_or(lost(name))?;
        let end = start.checked_add(len).ok_or(lost(name))?;
        if start < at {
            return Err(lost(name));
        }
        *slot = bytes.get(start..end).ok_or(FormatError::Truncated(name))?;
        at = end;
    }
    Ok(out)
}

impl<'a, T: Stored> GridView<'a, T> {
    /// Checks `bytes` whole as a frozen grid of `T` values and reads it in place.
    pub fn new(bytes: &'a [u8]) -> Result<Self, FormatError> {
        if bytes.as_ptr().align_offset(BLOCK) != 0 || !bytes.len().is_multiple_of(BLOCK) {
            return Err(FormatError::Misaligned);
        }
        let (h, _) =
            Header::ref_from_prefix(bytes).map_err(|_| FormatError::Truncated("header"))?;
        check_header::<T>(h, bytes.len())?;
        let [r, u, l, leaf, tile] = sections::<T>(h, bytes)?;
        let view = Self {
            roots: <[RootEntry]>::ref_from_bytes(r).map_err(|_| lost("root"))?,
            uppers: <[UpperNode]>::ref_from_bytes(u).map_err(|_| lost("upper node"))?,
            lowers: <[LowerNode]>::ref_from_bytes(l).map_err(|_| lost("lower node"))?,
            leaves: <[[T; LEAF_VOXELS]]>::ref_from_bytes(leaf).map_err(|_| lost("leaf value"))?,
            tiles: <[T]>::ref_from_bytes(tile).map_err(|_| lost("tile value"))?,
            background: T::read_from_prefix(h.background.as_bytes()).map_err(|_| lost("header"))?.0,
        };
        view.check_links()?;
        Ok(view)
    }

    /// Every root entry sorted, aligned and pointing within its section; every node's
    /// children and tiles within theirs.
    fn check_links(&self) -> Result<(), FormatError> {
        let keys = self.roots.iter().map(|e| e.key.map(|k| k.get()));
        let sorted = keys.clone().zip(keys.clone().skip(1)).all(|(a, b)| a < b);
        let aligned = keys.clone().all(|k| k.iter().all(|c| c & 4095 == 0));
        let pointing = self.roots.iter().all(|e| match e.kind.get() {
            ROOT_NODE => usize::try_from(e.index.get()).is_ok_and(|i| i < self.uppers.len()),
            ROOT_TILE => usize::try_from(e.index.get()).is_ok_and(|i| i < self.tiles.len()),
            _ => false,
        });
        if !(sorted && aligned && pointing) {
            return Err(lost("root"));
        }
        let n = [self.lowers.len(), self.tiles.len()];
        if !self
            .uppers
            .iter()
            .all(|u| links_fit(&u.children, &u.tiles, [u.first_child, u.first_tile], n))
        {
            return Err(lost("upper node"));
        }
        let n = [self.leaves.len(), self.tiles.len()];
        if !self
            .lowers
            .iter()
            .all(|l| links_fit(&l.children, &l.tiles, [l.first_child, l.first_tile], n))
        {
            return Err(lost("lower node"));
        }
        Ok(())
    }

    /// The number of leaves.
    pub fn leaf_count(&self) -> usize {
        self.leaves.len()
    }

    /// The number of tiles.
    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    /// The value outside the tree.
    pub fn background(&self) -> T {
        self.background
    }

    /// What holds the voxel.
    pub fn locate(&self, v: Coord) -> Result<ViewLocation, FormatError> {
        let key = TileLevel::Root.origin(v);
        let Ok(i) = self.roots.binary_search_by(|e| e.key.map(|k| k.get()).cmp(&key)) else {
            return Ok(ViewLocation::Background);
        };
        let e = self.roots.get(i).ok_or(lost("root"))?;
        if e.kind.get() == ROOT_TILE {
            return Ok(ViewLocation::Tile(e.index.get(), TileLevel::Root));
        }
        let u = self.uppers.get(e.index.get() as usize).ok_or(lost("root"))?;
        let p = place(local(v, 7, 5), 5);
        if is_set(&u.tiles, p) {
            return Ok(ViewLocation::Tile(
                nth(u.first_tile, &u.tiles, p, "upper node")?,
                TileLevel::Upper,
            ));
        }
        if !is_set(&u.children, p) {
            return Ok(ViewLocation::Background);
        }
        let l = self.lowers.get(nth(u.first_child, &u.children, p, "upper node")? as usize);
        let l = l.ok_or(lost("upper node"))?;
        let p = place(local(v, 3, 4), 4);
        Ok(if is_set(&l.children, p) {
            ViewLocation::Leaf(nth(l.first_child, &l.children, p, "lower node")?)
        } else if is_set(&l.tiles, p) {
            ViewLocation::Tile(nth(l.first_tile, &l.tiles, p, "lower node")?, TileLevel::Lower)
        } else {
            ViewLocation::Background
        })
    }

    /// Whether a node holds the voxel's region of `level`: an upper node its region of a
    /// root tile, a lower node its region of an upper node's tile, a leaf its region of a
    /// lower node's tile.
    pub(crate) fn holds_node(&self, level: TileLevel, v: Coord) -> Result<bool, FormatError> {
        let key = TileLevel::Root.origin(v);
        let Ok(i) = self.roots.binary_search_by(|e| e.key.map(|k| k.get()).cmp(&key)) else {
            return Ok(false);
        };
        let e = self.roots.get(i).ok_or(lost("root"))?;
        Ok(match level {
            TileLevel::Root => e.kind.get() != ROOT_TILE,
            TileLevel::Upper if e.kind.get() == ROOT_TILE => false,
            TileLevel::Upper => {
                let u = self.uppers.get(e.index.get() as usize).ok_or(lost("root"))?;
                is_set(&u.children, place(local(v, 7, 5), 5))
            }
            TileLevel::Lower => matches!(self.locate(v)?, ViewLocation::Leaf(_)),
        })
    }

    /// The value at the voxel: its leaf's, its tile's, or the background.
    pub fn get(&self, v: Coord) -> Result<T, FormatError> {
        match self.locate(v)? {
            ViewLocation::Leaf(n) => {
                let [x, y, z] = local(v, 0, 3).map(|c| c as usize);
                self.leaf(n)?.get(x + 8 * y + 64 * z).copied().ok_or(lost("leaf value"))
            }
            ViewLocation::Tile(t, _) => {
                self.tiles.get(t as usize).copied().ok_or(lost("tile value"))
            }
            ViewLocation::Background => Ok(self.background),
        }
    }

    /// A leaf's 512 values, x fastest.
    pub fn leaf(&self, n: u32) -> Result<&'a [T; LEAF_VOXELS], FormatError> {
        self.leaves.get(n as usize).ok_or(lost("leaf value"))
    }

    /// A tile's value, by its place among the view's tile values (`ViewLocation::Tile`).
    pub(crate) fn tile_value(&self, i: u32) -> Result<T, FormatError> {
        self.tiles.get(i as usize).copied().ok_or(lost("tile value"))
    }

    /// The view's tree and values as an owned topology and field, the topology numbering
    /// its leaves and tiles in its own order (ADR-0042). The nodes are walked once: a view
    /// in which a node, a leaf or a tile value is reached twice is refused as `Shared`.
    pub fn thaw(&self) -> Result<(Topology, Field<T>), FormatError> {
        let r = self.regions()?;
        let empty = Topology::new();
        let mut e = empty.edit();
        for o in &r.leaves {
            e.activate(*o).map_err(|_| lost("leaf value"))?;
        }
        for (level, o, _) in &r.tiles {
            e.add_tile(*level, *o).map_err(|_| lost("tile value"))?;
        }
        let t = e.commit().map_err(|_| lost("root"))?.0;
        let f = self.fill(&t, &r)?;
        Ok((t, f))
    }

    /// The view's values as a field on `t`, which must hold the view's leaves and tiles,
    /// as a topology thawed from a view of the same tree does; another is refused as
    /// `OtherTopology` (ADR-0042).
    pub fn thaw_on(&self, t: &Topology) -> Result<Field<T>, FormatError> {
        let r = self.regions()?;
        if t.leaf_count() != r.leaves.len() || t.tile_count() != r.tiles.len() {
            return Err(FormatError::OtherTopology);
        }
        self.fill(t, &r)
    }

    /// A field on `t` holding the view's values, each leaf and tile found in `t` at its
    /// origin; one not there is refused as `OtherTopology`. With as many leaves and tiles
    /// in `t` as in the view, each found at another origin, `t` holds them all.
    fn fill(&self, t: &Topology, r: &Regions) -> Result<Field<T>, FormatError> {
        let mut f = Field::new(t, self.background);
        for (n, o) in (0_u32..).zip(&r.leaves) {
            let Location::Leaf(m) = t.locate(*o) else { return Err(FormatError::OtherTopology) };
            *f.leaf_mut(t, m).map_err(|_| lost("leaf value"))? = *self.leaf(n)?;
        }
        for (level, o, i) in &r.tiles {
            let Location::Tile(m, _) = t.locate(*o) else {
                return Err(FormatError::OtherTopology);
            };
            if t.tile(m).map_err(|_| lost("tile value"))? != (*level, *o) {
                return Err(FormatError::OtherTopology);
            }
            f.set_tile(t, m, self.tile_value(*i)?).map_err(|_| lost("tile value"))?;
        }
        Ok(f)
    }

    /// Whether `other` holds the same tree: its root table and nodes are the same bytes,
    /// which do not depend on the value type (ADR-0032), and it holds as many leaves and
    /// tiles, so that neither holds values its nodes do not reach (ADR-0038). Two views of
    /// one tree thaw into one topology.
    pub fn same_topology<U: Stored>(&self, other: &GridView<'_, U>) -> bool {
        self.leaves.len() == other.leaves.len()
            && self.tiles.len() == other.tiles.len()
            && self.roots.as_bytes() == other.roots.as_bytes()
            && self.uppers.as_bytes() == other.uppers.as_bytes()
            && self.lowers.as_bytes() == other.lowers.as_bytes()
    }

    /// Every leaf's origin, by leaf number, and every tile's level and origin with its
    /// place among the tile values, found by walking the nodes once: a node, a leaf or a
    /// tile value reached twice is refused as `Shared`, so that the walk costs what the
    /// buffer holds, and a leaf never reached as `Inconsistent` (docs/formats.md §4.2,
    /// rule 11).
    pub(crate) fn regions(&self) -> Result<Regions, FormatError> {
        let mut leaves = vec![None; self.leaves.len()];
        let mut tiles = Vec::with_capacity(self.tiles.len());
        let mut seen = Seen::new(self.uppers.len(), self.lowers.len(), self.tiles.len());
        for e in self.roots {
            let key = e.key.map(|k| k.get());
            if e.kind.get() == ROOT_TILE {
                seen.tile(e.index.get())?;
                tiles.push((TileLevel::Root, key, e.index.get()));
                continue;
            }
            seen.upper(e.index.get())?;
            let u = self.uppers.get(e.index.get() as usize).ok_or(lost("root"))?;
            for p in 0..UPPER_PLACES {
                let at = offset(key, unplace(p, 5), TileLevel::Upper.edge_log2());
                if is_set(&u.tiles, p) {
                    let i = nth(u.first_tile, &u.tiles, p, "upper node")?;
                    seen.tile(i)?;
                    tiles.push((TileLevel::Upper, at, i));
                }
                if !is_set(&u.children, p) {
                    continue;
                }
                let n = nth(u.first_child, &u.children, p, "upper node")?;
                seen.lower(n)?;
                let l = self.lowers.get(n as usize).ok_or(lost("upper node"))?;
                for q in 0..LOWER_PLACES {
                    let leaf_at = offset(at, unplace(q, 4), TileLevel::Lower.edge_log2());
                    if is_set(&l.tiles, q) {
                        let i = nth(l.first_tile, &l.tiles, q, "lower node")?;
                        seen.tile(i)?;
                        tiles.push((TileLevel::Lower, leaf_at, i));
                    }
                    if is_set(&l.children, q) {
                        let i = nth(l.first_child, &l.children, q, "lower node")?;
                        let slot = leaves.get_mut(i as usize).ok_or(lost("lower node"))?;
                        if slot.replace(leaf_at).is_some() {
                            return Err(FormatError::Shared);
                        }
                    }
                }
            }
        }
        let leaves = leaves.into_iter().collect::<Option<Vec<_>>>().ok_or(lost("leaf value"))?;
        Ok(Regions { leaves, tiles })
    }
}

/// The upper nodes, lower nodes and tile values a walk has reached.
struct Seen {
    uppers: Vec<bool>,
    lowers: Vec<bool>,
    tiles: Vec<bool>,
}

impl Seen {
    fn new(uppers: usize, lowers: usize, tiles: usize) -> Self {
        Seen { uppers: vec![false; uppers], lowers: vec![false; lowers], tiles: vec![false; tiles] }
    }

    /// Marks entry `i` of `marks` reached, refusing one reached before.
    fn mark(marks: &mut [bool], i: u32, section: &'static str) -> Result<(), FormatError> {
        let mark = marks.get_mut(i as usize).ok_or(lost(section))?;
        if std::mem::replace(mark, true) {
            return Err(FormatError::Shared);
        }
        Ok(())
    }

    fn upper(&mut self, i: u32) -> Result<(), FormatError> {
        Self::mark(&mut self.uppers, i, "root")
    }

    fn lower(&mut self, i: u32) -> Result<(), FormatError> {
        Self::mark(&mut self.lowers, i, "upper node")
    }

    fn tile(&mut self, i: u32) -> Result<(), FormatError> {
        Self::mark(&mut self.tiles, i, "tile value")
    }
}

/// A view's leaves and tiles (`GridView::regions`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Regions {
    /// Each leaf's origin, by leaf number.
    pub(crate) leaves: Vec<Coord>,
    /// Each tile's level and origin, with its place among the tile values.
    pub(crate) tiles: Vec<(TileLevel, Coord, u32)>,
}

/// The local coordinates of Morton place `p` in a node of `2^bits` places a side: the
/// inverse of `place`.
fn unplace(p: usize, bits: u32) -> [u32; 3] {
    let mut out = [0_u32; 3];
    for b in 0..bits {
        for (axis, c) in out.iter_mut().enumerate() {
            *c |= u32::from((p >> (3 * b as usize + axis)) & 1 == 1) << b;
        }
    }
    out
}

/// The origin of the child at local coordinates `l` of a node at `origin`, each child
/// `2^child_log2` voxels a side.
fn offset(origin: Coord, l: [u32; 3], child_log2: u32) -> Coord {
    let mut out = origin;
    for (o, c) in out.iter_mut().zip(l) {
        *o = o.wrapping_add(c.cast_signed() << child_log2);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::Field;
    use crate::frozen::freeze;
    use crate::topology::Topology;

    /// Leaves and tiles of every level, in several root regions, some at negative
    /// coordinates.
    fn mixed() -> Topology {
        let empty = Topology::new();
        let mut e = empty.edit();
        for v in [[0, 0, 0], [8, 0, 0], [-9, 130, 7], [5000, -4097, 64], [127, 127, 127]] {
            e.activate(v).unwrap();
        }
        e.add_tile(TileLevel::Lower, [16, 0, 0]).unwrap();
        e.add_tile(TileLevel::Upper, [128, 0, 0]).unwrap();
        e.add_tile(TileLevel::Root, [0, 8192, 0]).unwrap();
        e.add_tile(TileLevel::Root, [-8192, 0, 0]).unwrap();
        e.commit().unwrap().0
    }

    #[test]
    fn walking_a_view_finds_the_topologys_leaves_and_tiles() {
        let t = mixed();
        let frozen = freeze(&t, &Field::new(&t, 0_u8)).unwrap();
        let view = frozen.view::<u8>().unwrap();
        let r = view.regions().unwrap();
        let leaves: Vec<Coord> = t.leaf_numbers().map(|n| t.leaf_origin(n).unwrap()).collect();
        assert_eq!(r.leaves, leaves);
        let mut tiles: Vec<(TileLevel, Coord)> = r.tiles.iter().map(|(l, o, _)| (*l, *o)).collect();
        let mut want: Vec<(TileLevel, Coord)> =
            t.tile_numbers().map(|n| t.tile(n).unwrap()).collect();
        tiles.sort_by_key(|(_, o)| *o);
        want.sort_by_key(|(_, o)| *o);
        assert_eq!(tiles, want);
        for (level, o, i) in &r.tiles {
            assert_eq!(view.locate(*o).unwrap(), ViewLocation::Tile(*i, *level), "at {o:?}");
        }
    }

    /// The `u32` at byte `at` of `b`.
    fn word(b: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
    }

    /// `mixed`'s frozen bytes with the second root entry of `kind` naming the first's
    /// index: an upper node, or a tile value, two entries share.
    fn shared(kind: u32) -> Vec<crate::frozen::Block> {
        let t = mixed();
        let mut b = freeze(&t, &Field::new(&t, 0_u8)).unwrap().as_bytes().to_vec();
        let (count, roots) = (word(&b, 16) as usize, word(&b, 36) as usize * 32);
        let at: Vec<usize> =
            (0..count).map(|i| roots + 32 * i).filter(|e| word(&b, e + 12) == kind).collect();
        let index = word(&b, at[0] + 16).to_le_bytes();
        b[at[1] + 16..at[1] + 20].copy_from_slice(&index);
        crate::frozen::aligned(&b)
    }

    #[test]
    fn a_node_or_tile_value_two_root_entries_share_is_refused_by_the_walk() {
        // Each entry names an index the counts allow, so the view's checks pass (§2.5);
        // walking the tree reaches the shared one twice (§4.2, rule 11).
        for kind in [ROOT_NODE, ROOT_TILE] {
            let b = shared(kind);
            let view = GridView::<u8>::new(b.as_bytes()).unwrap();
            assert_eq!(view.regions(), Err(FormatError::Shared), "kind {kind}");
            assert_eq!(view.thaw().err(), Some(FormatError::Shared), "kind {kind}");
            let t = mixed();
            assert_eq!(view.thaw_on(&t).err(), Some(FormatError::Shared), "kind {kind}");
        }
        let t = mixed();
        let frozen = freeze(&t, &Field::new(&t, 0_u8)).unwrap();
        assert!(frozen.view::<u8>().unwrap().regions().is_ok());
    }

    #[test]
    fn views_of_one_topology_share_it_whatever_their_type() {
        let t = mixed();
        let a = freeze(&t, &Field::new(&t, 0_u8)).unwrap();
        let b = freeze(&t, &Field::new(&t, [1.0_f32; 3])).unwrap();
        let mut e = t.edit();
        e.activate([300, 0, 0]).unwrap();
        let other = e.commit().unwrap().0;
        let c = freeze(&other, &Field::new(&other, 0_u8)).unwrap();
        let (a, b, c) =
            (a.view::<u8>().unwrap(), b.view::<[f32; 3]>().unwrap(), c.view::<u8>().unwrap());
        assert!(a.same_topology(&b));
        assert!(!a.same_topology(&c));
    }
}
