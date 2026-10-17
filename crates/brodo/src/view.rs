//! Reading a frozen grid in place (ARCHITECTURE.md §4.6, ADR-0016). This module parses
//! untrusted bytes: it is listed in `UNTRUSTED_INPUT`, indexes nothing (CODING.md Rule 3),
//! checks the whole buffer before any read, and is fuzzed (TESTING.md T6).

use zerocopy::little_endian::U32;
use zerocopy::{FromBytes, IntoBytes};

use crate::error::FormatError;
use crate::field::{LEAF_VOXELS, Value};
use crate::frozen::{
    BLOCK, Header, LowerNode, MAGIC, ROOT_NODE, ROOT_TILE, RootEntry, UpperNode, VERSION, local,
    place,
};
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

fn check_header<T: Value>(h: &Header, len: usize) -> Result<(), FormatError> {
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

impl<'a, T: Value> GridView<'a, T> {
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
}
