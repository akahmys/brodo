//! Reading a frozen grid in place (ARCHITECTURE.md §4.6, ADR-0016). This module parses
//! untrusted bytes: it is listed in `UNTRUSTED_INPUT`, indexes nothing (CODING.md Rule 3),
//! checks the whole buffer before any read, and is fuzzed (TESTING.md T6).

use zerocopy::little_endian::U32;
use zerocopy::{FromBytes, IntoBytes};

use crate::error::FormatError;
use crate::field::local_index;
use crate::field::{Field, LEAF_VOXELS, Stored};
use crate::frozen::{
    BLOCK, Header, LOWER_PLACES, LowerNode, MAGIC, ROOT_NODE, ROOT_TILE, RootEntry, UPPER_PLACES,
    UpperNode, VERSION, local, place,
};
use crate::topology::{Location, Topology, morton};
use crate::tree::{Coord, TileLevel};
use crate::tree::{LEAF_LOG2, LOWER_DIM_LOG2, LOWER_SPAN_LOG2, UPPER_DIM_LOG2, UPPER_SPAN_LOG2};

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
    /// The header's leaf count: `leaves`' length, or, in a view of nodes alone
    /// (`GridView::nodes`), the leaves its nodes link to.
    leaf_count: usize,
    tiles: &'a [T],
    background: T,
    version: u32,
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

/// The places set in a mask, in ascending order: as many from each word as it has bits set,
/// so that the walk ends however a word is read.
fn set_places(mask: &[U32]) -> impl Iterator<Item = usize> + '_ {
    mask.iter().enumerate().flat_map(|(i, w)| {
        let mut bits = w.get();
        std::iter::repeat_with(move || {
            let b = bits.trailing_zeros();
            bits &= bits.wrapping_sub(1);
            i * 32 + b as usize
        })
        .take(w.get().count_ones() as usize)
    })
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

/// A node (docs/formats.md §2.5, rule 11): holding a child or a tile, its first
/// child and first tile the counts so far, or 0 for none, the counts then moved past its
/// own; `None` where it breaks the rule.
fn consecutive(
    children: &[U32],
    tiles: &[U32],
    first: [U32; 2],
    counts: [&mut u32; 2],
) -> Option<()> {
    let held = [ones(children), ones(tiles)];
    if held == [0, 0] {
        return None;
    }
    for ((n, f), count) in held.into_iter().zip(first).zip(counts) {
        let expected = if n == 0 { 0 } else { *count };
        if f.get() != expected {
            return None;
        }
        *count = count.checked_add(n)?;
    }
    Some(())
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

/// The sections' bytes, each where the header says, in order, none overlapping, each
/// found by `find` from its place among them and its byte range.
fn sections<'a, T>(
    h: &Header,
    find: impl Fn(usize, std::ops::Range<usize>, &'static str) -> Result<&'a [u8], FormatError>,
) -> Result<[&'a [u8]; 5], FormatError> {
    let sizes = [
        size_of::<RootEntry>(),
        size_of::<UpperNode>(),
        size_of::<LowerNode>(),
        size_of::<[T; LEAF_VOXELS]>(),
        size_of::<T>(),
    ];
    let mut at = size_of::<Header>();
    let mut out: [&[u8]; 5] = [&[]; 5];
    for (i, ((((slot, size), count), offset), name)) in
        out.iter_mut().zip(sizes).zip(h.counts).zip(h.offsets).zip(NAMES).enumerate()
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
        *slot = find(i, start..end, name)?;
        at = end;
    }
    Ok(out)
}

impl<'a, T: Stored> GridView<'a, T> {
    /// Checks `bytes` whole as a frozen grid of `T` values and reads it in place, as
    /// docs/formats.md §2.5 states; a broken rule is refused with the `FormatError` whose
    /// documentation names it, and of several, which is named is not promised.
    pub fn new(bytes: &'a [u8]) -> Result<Self, FormatError> {
        if bytes.as_ptr().align_offset(BLOCK) != 0 || !bytes.len().is_multiple_of(BLOCK) {
            return Err(FormatError::Misaligned);
        }
        let (h, _) =
            Header::ref_from_prefix(bytes).map_err(|_| FormatError::Truncated("header"))?;
        check_header::<T>(h, bytes.len())?;
        let found =
            sections::<T>(h, |_, r, name| bytes.get(r).ok_or(FormatError::Truncated(name)))?;
        Self::from_sections(h, found)
    }

    /// Checks a frozen buffer of `len` bytes of `T` values whose leaf values section is
    /// left out: `head`, its bytes before that section, and `tail`, its bytes after it, each
    /// starting on a 32-byte boundary. The view checks as `new` does, the leaf values aside,
    /// and holds no leaf values (docs/formats.md §8.6).
    #[cfg(feature = "archive")]
    pub(crate) fn nodes(head: &'a [u8], tail: &'a [u8], len: usize) -> Result<Self, FormatError> {
        let tail_at = len.checked_sub(tail.len()).ok_or(lost("header"))?;
        let aligned = |b: &[u8]| b.as_ptr().align_offset(BLOCK) == 0;
        if !(aligned(head) && aligned(tail)) || !head.len().is_multiple_of(BLOCK) {
            return Err(FormatError::Misaligned);
        }
        let (h, _) = Header::ref_from_prefix(head).map_err(|_| FormatError::Truncated("header"))?;
        check_header::<T>(h, len)?;
        let found = sections::<T>(h, |i, r, name| {
            if i == 3 {
                // The leaf values: exactly what lies between the two, held as no bytes at
                // the end of `head`, which is aligned as they would be.
                let between = r == (head.len()..tail_at);
                return head.get(head.len()..).filter(|_| between).ok_or(lost(name));
            }
            let held = if r.end <= head.len() {
                head.get(r)
            } else {
                let start = r.start.checked_sub(tail_at);
                start.zip(r.end.checked_sub(tail_at)).and_then(|(s, e)| tail.get(s..e))
            };
            held.ok_or(FormatError::Truncated(name))
        })?;
        Self::from_sections(h, found)
    }

    /// The view of the sections `found` holds, its links checked.
    fn from_sections(h: &Header, found: [&'a [u8]; 5]) -> Result<Self, FormatError> {
        let [r, u, l, leaf, tile] = found;
        let view = Self {
            roots: <[RootEntry]>::ref_from_bytes(r).map_err(|_| lost("root"))?,
            uppers: <[UpperNode]>::ref_from_bytes(u).map_err(|_| lost("upper node"))?,
            lowers: <[LowerNode]>::ref_from_bytes(l).map_err(|_| lost("lower node"))?,
            leaves: <[[T; LEAF_VOXELS]]>::ref_from_bytes(leaf).map_err(|_| lost("leaf value"))?,
            leaf_count: usize::try_from(h.counts[3].get()).map_err(|_| lost("leaf value"))?,
            tiles: <[T]>::ref_from_bytes(tile).map_err(|_| lost("tile value"))?,
            background: T::read_from_prefix(h.background.as_bytes()).map_err(|_| lost("header"))?.0,
            version: h.version.get(),
        };
        view.check_links()?;
        view.check_writer(h)?;
        Ok(view)
    }

    /// The writer's rules beyond the links (docs/formats.md §2.5, rules 7 to 11): every rule
    /// the writer keeps, so that each node, leaf and tile value is reached from one place.
    fn check_writer(&self, h: &Header) -> Result<(), FormatError> {
        let zero = |b: &[u8]| b.iter().all(|x| *x == 0);
        let background = h.background.as_bytes().get(size_of::<T>()..).unwrap_or_default();
        let padded = zero(h.pad.as_bytes())
            && zero(background)
            && self.roots.iter().all(|e| zero(e.pad.as_bytes()))
            && self.uppers.iter().all(|u| zero(u.pad.as_bytes()))
            && self.lowers.iter().all(|l| zero(l.pad.as_bytes()));
        if !padded {
            return Err(lost("padding"));
        }
        self.check_places(h)?;
        self.check_order()
    }

    /// Rule 8: each section at the block right after the one before, the buffer ending at
    /// the last; and rule 9: root tiles numbered in table order.
    fn check_places(&self, h: &Header) -> Result<(), FormatError> {
        let lens = [
            self.roots.as_bytes().len(),
            self.uppers.as_bytes().len(),
            self.lowers.as_bytes().len(),
            self.leaf_count.saturating_mul(size_of::<[T; LEAF_VOXELS]>()),
            self.tiles.as_bytes().len(),
        ];
        let mut at = size_of::<Header>().div_ceil(BLOCK);
        for ((offset, len), name) in h.offsets.iter().zip(lens).zip(NAMES) {
            if usize::try_from(offset.get()).ok() != Some(at) {
                return Err(lost(name));
            }
            at = at.saturating_add(len.div_ceil(BLOCK));
        }
        if usize::try_from(h.blocks.get()).ok() != Some(at) {
            return Err(lost("header"));
        }
        let mut tiles = 0_u32;
        for e in self.roots.iter().filter(|e| e.kind.get() == ROOT_TILE) {
            if e.index.get() != tiles {
                return Err(lost("root"));
            }
            tiles = tiles.saturating_add(1);
        }
        Ok(())
    }

    /// Rule 9's second half and rules 10 and 11: each upper node named by one root entry,
    /// in ascending Morton key of its origin, every node holding a child or a tile, and each
    /// node's first child and first tile the count of those before it, so that each lower
    /// node, leaf and tile value is reached from one place alone; the lower nodes' and
    /// leaves' Morton order follows, their origins being their parents' and their places.
    fn check_order(&self) -> Result<(), FormatError> {
        let mut origins: Vec<Option<u128>> = vec![None; self.uppers.len()];
        for e in self.roots.iter().filter(|e| e.kind.get() == ROOT_NODE) {
            let slot = origins.get_mut(e.index.get() as usize).ok_or(lost("root"))?;
            if slot.replace(morton(e.key.map(|k| k.get()))).is_some() {
                return Err(lost("root"));
            }
        }
        let mortons: Vec<u128> = origins.into_iter().collect::<Option<_>>().ok_or(lost("root"))?;
        if !mortons.iter().zip(mortons.iter().skip(1)).all(|(a, b)| a < b) {
            return Err(lost("upper node"));
        }
        let root_tiles = self.roots.iter().filter(|e| e.kind.get() == ROOT_TILE).count();
        let mut tiles = u32::try_from(root_tiles).map_err(|_| lost("root"))?;
        let mut lowers = 0_u32;
        for u in self.uppers {
            consecutive(
                &u.children,
                &u.tiles,
                [u.first_child, u.first_tile],
                [&mut lowers, &mut tiles],
            )
            .ok_or(lost("upper node"))?;
        }
        let mut leaves = 0_u32;
        for l in self.lowers {
            consecutive(
                &l.children,
                &l.tiles,
                [l.first_child, l.first_tile],
                [&mut leaves, &mut tiles],
            )
            .ok_or(lost("lower node"))?;
        }
        let ends =
            [(lowers, self.lowers.len()), (leaves, self.leaf_count), (tiles, self.tiles.len())];
        if ends.iter().all(|(n, len)| usize::try_from(*n).ok() == Some(*len)) {
            Ok(())
        } else {
            Err(lost("tile value"))
        }
    }

    /// Every root entry sorted, aligned and pointing within its section; every node's
    /// children and tiles within theirs.
    fn check_links(&self) -> Result<(), FormatError> {
        let keys = self.roots.iter().map(|e| e.key.map(|k| k.get()));
        let sorted = keys.clone().zip(keys.clone().skip(1)).all(|(a, b)| a < b);
        let aligned = keys.clone().all(|k| k.iter().all(|c| c & ((1 << UPPER_SPAN_LOG2) - 1) == 0));
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
        let n = [self.leaf_count, self.tiles.len()];
        if !self
            .lowers
            .iter()
            .all(|l| links_fit(&l.children, &l.tiles, [l.first_child, l.first_tile], n))
        {
            return Err(lost("lower node"));
        }
        Ok(())
    }

    /// The layout version of the bytes: 2 (docs/formats.md §2).
    pub fn version(&self) -> u32 {
        self.version
    }

    /// The bounds the view's leaves and tiles cover, by a walk of its nodes' set places
    /// alone, each node's children numbered from its first in place order.
    pub(crate) fn bounds(&self) -> Result<crate::meta::Bounds, FormatError> {
        let mut b = crate::meta::Bounds::Empty;
        for e in self.roots {
            let key = e.key.map(|k| k.get());
            if e.kind.get() == ROOT_TILE {
                b = b.with_cube(key, TileLevel::Root.edge() - 1);
                continue;
            }
            let u = self.uppers.get(e.index.get() as usize).ok_or(lost("root"))?;
            let at = |p| offset(key, unplace(p, 5), TileLevel::Upper.edge_log2());
            for p in set_places(&u.tiles) {
                b = b.with_cube(at(p), TileLevel::Upper.edge() - 1);
            }
            for (n, p) in (u.first_child.get() as usize..).zip(set_places(&u.children)) {
                let l = self.lowers.get(n).ok_or(lost("upper node"))?;
                let at = |q| offset(at(p), unplace(q, 4), TileLevel::Lower.edge_log2());
                for q in set_places(&l.tiles) {
                    b = b.with_cube(at(q), TileLevel::Lower.edge() - 1);
                }
                for q in set_places(&l.children) {
                    b = b.with_cube(at(q), (1 << LEAF_LOG2) - 1);
                }
            }
        }
        Ok(b)
    }

    /// The number of leaves.
    pub fn leaf_count(&self) -> usize {
        self.leaf_count
    }

    /// The number of tiles.
    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    /// The value outside the tree.
    pub fn background(&self) -> T {
        self.background
    }

    /// The root entry of the voxel's 4096-region, if any.
    fn root_entry(&self, v: Coord) -> Option<&'a RootEntry> {
        let key = TileLevel::Root.origin(v);
        let i = self.roots.binary_search_by(|e| e.key.map(|k| k.get()).cmp(&key)).ok()?;
        self.roots.get(i)
    }

    /// What holds the voxel.
    pub fn locate(&self, v: Coord) -> Result<ViewLocation, FormatError> {
        let Some(e) = self.root_entry(v) else { return Ok(ViewLocation::Background) };
        if e.kind.get() == ROOT_TILE {
            return Ok(ViewLocation::Tile(e.index.get(), TileLevel::Root));
        }
        let u = self.uppers.get(e.index.get() as usize).ok_or(lost("root"))?;
        let p = place(local(v, LOWER_SPAN_LOG2, UPPER_DIM_LOG2), UPPER_DIM_LOG2);
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
        let p = place(local(v, LEAF_LOG2, LOWER_DIM_LOG2), LOWER_DIM_LOG2);
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
        let Some(e) = self.root_entry(v) else { return Ok(false) };
        Ok(match level {
            TileLevel::Root => e.kind.get() != ROOT_TILE,
            TileLevel::Upper if e.kind.get() == ROOT_TILE => false,
            TileLevel::Upper => {
                let u = self.uppers.get(e.index.get() as usize).ok_or(lost("root"))?;
                is_set(
                    &u.children,
                    place(local(v, LOWER_SPAN_LOG2, UPPER_DIM_LOG2), UPPER_DIM_LOG2),
                )
            }
            TileLevel::Lower => matches!(self.locate(v)?, ViewLocation::Leaf(_)),
        })
    }

    /// The value at the voxel: its leaf's, its tile's, or the background.
    pub fn get(&self, v: Coord) -> Result<T, FormatError> {
        match self.locate(v)? {
            ViewLocation::Leaf(n) => {
                self.leaf(n)?.get(local_index(v)).copied().ok_or(lost("leaf value"))
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
    /// its leaves and tiles in its own order (ADR-0042), the nodes walked once.
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
        self.leaf_count == other.leaf_count
            && self.tiles.len() == other.tiles.len()
            && self.roots.as_bytes() == other.roots.as_bytes()
            && self.uppers.as_bytes() == other.uppers.as_bytes()
            && self.lowers.as_bytes() == other.lowers.as_bytes()
    }

    /// Every leaf's origin, by leaf number, and every tile's level and origin with its
    /// place among the tile values, found by walking the nodes once; the writer's rules,
    /// checked when the view was made, reach each from one place (docs/formats.md §2.5,
    /// rule 11), so that the walk costs what the buffer holds.
    pub(crate) fn regions(&self) -> Result<Regions, FormatError> {
        let mut leaves = vec![None; self.leaf_count];
        let mut tiles = Vec::with_capacity(self.tiles.len());
        for e in self.roots {
            let key = e.key.map(|k| k.get());
            if e.kind.get() == ROOT_TILE {
                tiles.push((TileLevel::Root, key, e.index.get()));
                continue;
            }
            let u = self.uppers.get(e.index.get() as usize).ok_or(lost("root"))?;
            // The places are walked in order, so each child's and tile's number is its
            // node's first plus those met so far, not counted again from the masks.
            let (mut tile, mut child) = (Next::from(u.first_tile), Next::from(u.first_child));
            for p in 0..UPPER_PLACES {
                let at = offset(key, unplace(p, 5), TileLevel::Upper.edge_log2());
                if is_set(&u.tiles, p) {
                    let i = tile.take("upper node")?;
                    tiles.push((TileLevel::Upper, at, i));
                }
                if !is_set(&u.children, p) {
                    continue;
                }
                let n = child.take("upper node")?;
                let l = self.lowers.get(n as usize).ok_or(lost("upper node"))?;
                let (mut tile, mut leaf) = (Next::from(l.first_tile), Next::from(l.first_child));
                for q in 0..LOWER_PLACES {
                    let leaf_at = offset(at, unplace(q, 4), TileLevel::Lower.edge_log2());
                    if is_set(&l.tiles, q) {
                        let i = tile.take("lower node")?;
                        tiles.push((TileLevel::Lower, leaf_at, i));
                    }
                    if is_set(&l.children, q) {
                        let i = leaf.take("lower node")?;
                        let slot = leaves.get_mut(i as usize).ok_or(lost("lower node"))?;
                        *slot = Some(leaf_at);
                    }
                }
            }
        }
        let leaves = leaves.into_iter().collect::<Option<Vec<_>>>().ok_or(lost("leaf value"))?;
        Ok(Regions { leaves, tiles })
    }
}

#[cfg(feature = "archive")]
impl<T: Stored> GridView<'_, T> {
    /// The number and origin of every leaf whose voxels meet the box from `min` to `max`,
    /// inclusive, in leaf number order, found by walking only the nodes that meet it, each
    /// reached from one place (docs/formats.md §2.5, rule 11), so that the walk costs what
    /// the nodes it walks hold.
    pub(crate) fn leaves_meeting(
        &self,
        min: Coord,
        max: Coord,
    ) -> Result<Vec<(u32, Coord)>, FormatError> {
        let meets = |o: Coord, level: TileLevel| {
            let far = i64::from(level.edge()) - 1;
            (0..3).all(|k| {
                let (o, lo, hi) = (o.get(k), min.get(k), max.get(k));
                o.zip(lo).zip(hi).is_some_and(|((o, lo), hi)| {
                    i64::from(*o) <= i64::from(*hi) && i64::from(*o) + far >= i64::from(*lo)
                })
            })
        };
        let mut out = Vec::new();
        for e in self.roots {
            let key = e.key.map(|k| k.get());
            if e.kind.get() == ROOT_TILE || !meets(key, TileLevel::Root) {
                continue;
            }
            let u = self.uppers.get(e.index.get() as usize).ok_or(lost("root"))?;
            let mut child = Next::from(u.first_child);
            for p in 0..UPPER_PLACES {
                if !is_set(&u.children, p) {
                    continue;
                }
                let n = child.take("upper node")?;
                let at = offset(key, unplace(p, 5), TileLevel::Upper.edge_log2());
                if !meets(at, TileLevel::Upper) {
                    continue;
                }
                let l = self.lowers.get(n as usize).ok_or(lost("upper node"))?;
                let mut leaf = Next::from(l.first_child);
                for q in 0..LOWER_PLACES {
                    if is_set(&l.children, q) {
                        let i = leaf.take("lower node")?;
                        let o = offset(at, unplace(q, 4), TileLevel::Lower.edge_log2());
                        if meets(o, TileLevel::Lower) {
                            out.push((i, o));
                        }
                    }
                }
            }
        }
        out.sort_unstable();
        Ok(out)
    }
}

/// The number of a node's next child or tile in a walk of its places in order: its first
/// plus those met so far.
struct Next(u32);

impl From<U32> for Next {
    fn from(first: U32) -> Self {
        Next(first.get())
    }
}

impl Next {
    fn take(&mut self, what: &'static str) -> Result<u32, FormatError> {
        let n = self.0;
        self.0 = n.checked_add(1).ok_or(lost(what))?;
        Ok(n)
    }
}

/// A view's leaves and tiles (`GridView::regions`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
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

    /// `mixed`'s frozen bytes with the second root entry of `kind` naming the first's index:
    /// an upper node, or a tile value, two entries share.
    fn shared(kind: u32) -> Vec<u8> {
        let t = mixed();
        let mut b = freeze(&t, &Field::new(&t, 0_u8)).unwrap().as_bytes().to_vec();
        let (count, roots) = (word(&b, 16) as usize, word(&b, 36) as usize * 32);
        let at: Vec<usize> =
            (0..count).map(|i| roots + 32 * i).filter(|e| word(&b, e + 12) == kind).collect();
        let index = word(&b, at[0] + 16).to_le_bytes();
        b[at[1] + 16..at[1] + 20].copy_from_slice(&index);
        b
    }

    #[test]
    fn a_node_or_tile_value_two_root_entries_share_is_refused() {
        // Each entry names an index the counts allow, so the links pass (§2.5); the writer's
        // rules 9 and 11 refuse the sharing, so that no walk reaches one twice.
        for kind in [ROOT_NODE, ROOT_TILE] {
            refused("a shared root entry", &shared(kind), FormatError::Inconsistent("root"));
        }
        let t = mixed();
        let frozen = freeze(&t, &Field::new(&t, 0_u8)).unwrap();
        assert!(frozen.view::<u8>().unwrap().regions().is_ok());
    }

    /// Bytes with one writer rule broken refused for it (docs/formats.md §2.5, rules 7 to 11).
    fn refused(what: &str, b: &[u8], expected: FormatError) {
        let at = GridView::<u8>::new(crate::frozen::aligned(b).as_bytes()).map(|_| ());
        assert_eq!(at, Err(expected), "{what}");
    }

    #[test]
    fn each_writer_rule_broken_is_refused() {
        let t = mixed();
        let good = freeze(&t, &Field::new(&t, 0_u8)).unwrap().as_bytes().to_vec();
        assert_eq!(word(&good, 4), 2);
        assert!(GridView::<u8>::new(crate::frozen::aligned(&good).as_bytes()).is_ok());
        let section = |b: &[u8], i: usize| word(b, 36 + 4 * i) as usize * 32;
        let padding = FormatError::Inconsistent("padding");
        let mut damaged: Vec<(&str, Vec<u8>, FormatError)> = Vec::new();
        for (what, at) in [
            ("header padding", 90),
            ("background beyond its size", 61),
            ("root padding", section(&good, 0) + 24),
            ("upper padding", section(&good, 1) + 8200),
            ("lower padding", section(&good, 2) + 1032),
        ] {
            let mut b = good.clone();
            b[at] = 1;
            damaged.push((what, b, padding));
        }
        // A block after the last section, the header's length counting it.
        let mut b = good.clone();
        b.extend([0; 32]);
        let blocks = word(&b, 56) + 1;
        b[56..60].copy_from_slice(&blocks.to_le_bytes());
        damaged.push(("a block after the last section", b, FormatError::Inconsistent("header")));
        // The two root tiles' places swapped, and two upper nodes' root entries swapped.
        for (what, kind, error) in [
            ("root tiles out of table order", ROOT_TILE, FormatError::Inconsistent("root")),
            ("upper nodes out of Morton order", ROOT_NODE, FormatError::Inconsistent("upper node")),
        ] {
            let mut b = good.clone();
            let entries: Vec<usize> = (0..word(&b, 16) as usize)
                .map(|i| section(&b, 0) + 32 * i)
                .filter(|e| word(&b, e + 12) == kind)
                .collect();
            let (x, y) = (word(&b, entries[0] + 16), word(&b, entries[1] + 16));
            b[entries[0] + 16..entries[0] + 20].copy_from_slice(&y.to_le_bytes());
            b[entries[1] + 16..entries[1] + 20].copy_from_slice(&x.to_le_bytes());
            damaged.push((what, b, error));
        }
        // The second lower node's first child, 0 where its children follow the first's.
        let mut b = good.clone();
        let at = section(&b, 2) + 1056 + 1024;
        assert_ne!(word(&b, at), 0);
        b[at..at + 4].fill(0);
        damaged.push((
            "a first child not the count before it",
            b,
            FormatError::Inconsistent("lower node"),
        ));
        for (what, b, expected) in damaged {
            refused(what, &b, expected);
        }
    }

    /// A lower node holding neither child nor tile, its counts and links else consistent,
    /// is refused (docs/formats.md §2.5, rule 11).
    #[test]
    fn an_empty_node_is_refused() {
        let empty = Topology::new();
        let mut e = empty.edit();
        e.activate([0, 0, 0]).unwrap();
        e.add_tile(TileLevel::Lower, [128, 0, 0]).unwrap();
        let t = e.commit().unwrap().0;
        let mut b = freeze(&t, &Field::new(&t, 0_u8)).unwrap().as_bytes().to_vec();
        assert_eq!((word(&b, 24), word(&b, 32)), (2, 1));
        // The second lower node's one tile cleared, and the tile values' block dropped.
        let tiles = word(&b, 44) as usize * 32 + 1056 + 512;
        b[tiles..tiles + 512].fill(0);
        b[32..36].fill(0);
        let blocks = word(&b, 56) - 1;
        b[56..60].copy_from_slice(&blocks.to_le_bytes());
        b.truncate(b.len() - 32);
        refused("an empty lower node", &b, FormatError::Inconsistent("lower node"));
    }

    /// A view of the nodes alone refuses a part off a 32-byte boundary, the other aligned,
    /// and a head not of whole blocks.
    #[cfg(feature = "archive")]
    #[test]
    fn a_view_of_the_nodes_refuses_parts_off_their_blocks() {
        let t = mixed();
        let frozen = freeze(&t, &Field::new(&t, 0_u8)).unwrap();
        let b = frozen.as_bytes();
        let len = b.len();
        for (head, tail) in [(&b[..64], &b[len - 31..]), (&b[..33], &b[len - 32..])] {
            assert_eq!(
                GridView::<u8>::nodes(head, tail, len).err(),
                Some(FormatError::Misaligned),
                "a head of {} bytes, a tail of {}",
                head.len(),
                tail.len()
            );
        }
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
