//! The frozen layout (ARCHITECTURE.md §4.6, ADR-0016) and `freeze`, which packs a topology
//! and a field into one 32-byte aligned buffer that a `GridView` reads in place.
//!
//! Layout version 1, every structure 32-byte aligned, in little-endian `u32` words, offsets
//! in 32-byte units: the header; the root table, sorted by key; upper nodes, then lower
//! nodes, each in the Morton order of its origin; leaf values in leaf-number order; tile
//! values, root tiles first, then each upper node's, then each lower node's. A node's masks
//! number its children's places in Morton order: a child is its node's first child plus
//! the set bits below its place.

use std::collections::BTreeMap;

use zerocopy::little_endian::{I32, U32};
use zerocopy::{FromBytes, FromZeros, Immutable, IntoBytes, KnownLayout};

use crate::error::{FieldError, FreezeError};
use crate::field::{Field, Value};
use crate::topology::{Location, Topology, morton};
use crate::tree::{Coord, TileLevel};

#[cfg(target_endian = "big")]
compile_error!("the frozen layout stores values little-endian, as brodo's targets are");

/// "BRDG", read as a little-endian `u32`.
pub(crate) const MAGIC: u32 = u32::from_le_bytes(*b"BRDG");
/// The layout version this build writes and reads (ADR-0016).
pub(crate) const VERSION: u32 = 1;
/// Bytes in a block, the unit of offsets and alignment.
pub(crate) const BLOCK: usize = 32;
/// Places in an upper node, and its mask words.
pub(crate) const UPPER_PLACES: usize = 32 * 32 * 32;
/// Places in a lower node, and its mask words.
pub(crate) const LOWER_PLACES: usize = 16 * 16 * 16;
/// The kinds of root entry.
pub(crate) const ROOT_TILE: u32 = 0;
/// A root entry naming an upper node.
pub(crate) const ROOT_NODE: u32 = 1;

/// The header: what the buffer holds and where.
#[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Debug, Clone, Copy)]
#[repr(C, align(32))]
pub(crate) struct Header {
    pub(crate) magic: U32,
    pub(crate) version: U32,
    pub(crate) value_type: U32,
    pub(crate) value_size: U32,
    /// Roots, upper nodes, lower nodes, leaves, tiles.
    pub(crate) counts: [U32; 5],
    /// The same sections' offsets, in blocks.
    pub(crate) offsets: [U32; 5],
    /// The buffer's length, in blocks.
    pub(crate) blocks: U32,
    /// The background value's bytes, zero beyond its size: up to 24, `[f64; 3]`'s. Its
    /// last three words were padding before `[f64; 3]`, zero for every smaller type, so
    /// that their bytes are as before (ADR-0029).
    pub(crate) background: [U32; 6],
    pub(crate) pad: [U32; 3],
}

/// One entry of the root table: an upper node or a root tile, by its region's origin.
#[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Debug, Clone, Copy)]
#[repr(C, align(32))]
pub(crate) struct RootEntry {
    pub(crate) key: [I32; 3],
    /// `ROOT_TILE` or `ROOT_NODE`.
    pub(crate) kind: U32,
    /// The upper node's number, or the tile's.
    pub(crate) index: U32,
    pub(crate) pad: [U32; 3],
}

/// An upper node: masks over its 32³ places, in Morton order.
#[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Debug, Clone, Copy)]
#[repr(C, align(32))]
pub(crate) struct UpperNode {
    pub(crate) children: [U32; UPPER_PLACES / 32],
    pub(crate) tiles: [U32; UPPER_PLACES / 32],
    pub(crate) first_child: U32,
    pub(crate) first_tile: U32,
    pub(crate) pad: [U32; 6],
}

/// A lower node: masks over its 16³ places, in Morton order.
#[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Debug, Clone, Copy)]
#[repr(C, align(32))]
pub(crate) struct LowerNode {
    pub(crate) children: [U32; LOWER_PLACES / 32],
    pub(crate) tiles: [U32; LOWER_PLACES / 32],
    pub(crate) first_child: U32,
    pub(crate) first_tile: U32,
    pub(crate) pad: [U32; 6],
}

/// A block of the buffer, so that the buffer is 32-byte aligned.
#[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Debug, Clone, Copy)]
#[repr(C, align(32))]
pub(crate) struct Block([u8; BLOCK]);

/// A 32-byte aligned copy of `bytes`, zero-padded to whole blocks.
pub(crate) fn aligned(bytes: &[u8]) -> Vec<Block> {
    let mut out = vec![Block([0; BLOCK]); bytes.len().div_ceil(BLOCK)];
    out.as_mut_bytes()[..bytes.len()].copy_from_slice(bytes);
    out
}

/// A frozen grid: the bytes of layout version 1, 32-byte aligned.
#[derive(Debug, Clone)]
pub struct FrozenGrid {
    blocks: Vec<Block>,
}

impl FrozenGrid {
    /// The buffer's bytes; their start is 32-byte aligned.
    pub fn as_bytes(&self) -> &[u8] {
        self.blocks.as_bytes()
    }

    /// A view of this grid's values of type `T`.
    pub fn view<T: Value>(
        &self,
    ) -> Result<crate::view::GridView<'_, T>, crate::error::FormatError> {
        crate::view::GridView::new(self.as_bytes())
    }
}

/// The place of a child in a node, from its local coordinates, `bits` per axis: the bits
/// interleaved, x lowest, as in the topology's Morton order.
pub(crate) fn place(local: [u32; 3], bits: u32) -> usize {
    let mut p = 0_usize;
    for b in 0..bits {
        for (axis, c) in local.iter().enumerate() {
            p |= (((c >> b) & 1) as usize) << (3 * b as usize + axis);
        }
    }
    p
}

/// The local coordinates of `v` in a node whose children span `2^child_log2` voxels.
pub(crate) fn local(v: Coord, child_log2: u32, dim_log2: u32) -> [u32; 3] {
    v.map(|c| (c.cast_unsigned() >> child_log2) & ((1 << dim_log2) - 1))
}

fn set(mask: &mut [U32], p: usize) {
    let w = &mut mask[p / 32];
    *w = U32::new(w.get() | (1 << (p % 32)));
}

fn to_u32(n: usize) -> Result<u32, FreezeError> {
    u32::try_from(n).map_err(|_| FreezeError::TooLarge)
}

/// The structure `freeze` lays out, in section order.
struct Plan {
    roots: Vec<RootEntry>,
    uppers: Vec<UpperNode>,
    lowers: Vec<LowerNode>,
    /// Tile values' sources, by topology tile number, in section order.
    tiles: Vec<u32>,
}

fn origin(v: Coord, level: TileLevel) -> Coord {
    level.origin(v)
}

/// Builds the nodes and the tile order from the topology's leaves and tiles.
fn plan(t: &Topology) -> Result<Plan, FreezeError> {
    let leaves: Vec<Coord> =
        t.leaf_numbers().map(|n| t.leaf_origin(n)).collect::<Result<_, _>>()?;
    let tiles: Vec<(TileLevel, Coord)> =
        t.tile_numbers().map(|n| t.tile(n)).collect::<Result<_, _>>()?;
    let by_morton = |set: Vec<Coord>| -> Vec<Coord> {
        let mut v = set;
        v.sort_by_cached_key(|o| morton(*o));
        v.dedup();
        v
    };
    let below_root = leaves
        .iter()
        .copied()
        .chain(tiles.iter().filter(|(l, _)| *l != TileLevel::Root).map(|(_, o)| *o));
    let upper_keys = by_morton(below_root.map(|o| origin(o, TileLevel::Root)).collect());
    let lower_src = leaves
        .iter()
        .copied()
        .chain(tiles.iter().filter(|(l, _)| *l == TileLevel::Lower).map(|(_, o)| *o));
    let lower_keys = by_morton(lower_src.map(|o| origin(o, TileLevel::Upper)).collect());
    let mut uppers: Vec<UpperNode> = upper_keys.iter().map(|_| UpperNode::new_zeroed()).collect();
    let mut lowers: Vec<LowerNode> = lower_keys.iter().map(|_| LowerNode::new_zeroed()).collect();
    let upper_no: BTreeMap<Coord, usize> =
        upper_keys.iter().enumerate().map(|(i, k)| (*k, i)).collect();
    let lower_no: BTreeMap<Coord, usize> =
        lower_keys.iter().enumerate().map(|(i, k)| (*k, i)).collect();
    link_children(&mut uppers, &mut lowers, &leaves, &lower_keys, &upper_no, &lower_no)?;
    let roots = roots(&upper_keys, &tiles)?;
    let order = tile_order(t, &tiles, &mut uppers, &mut lowers, &upper_no, &lower_no)?;
    Ok(Plan { roots, uppers, lowers, tiles: order })
}

/// Marks each lower node in its upper node and each leaf in its lower node, and records
/// each node's first child: children are consecutive, in Morton order.
fn link_children(
    uppers: &mut [UpperNode],
    lowers: &mut [LowerNode],
    leaves: &[Coord],
    lower_keys: &[Coord],
    upper_no: &BTreeMap<Coord, usize>,
    lower_no: &BTreeMap<Coord, usize>,
) -> Result<(), FreezeError> {
    let mut seen = vec![false; uppers.len()];
    for (i, k) in lower_keys.iter().enumerate() {
        let u = upper_no[&origin(*k, TileLevel::Root)];
        if !seen[u] {
            seen[u] = true;
            uppers[u].first_child = U32::new(to_u32(i)?);
        }
        set(&mut uppers[u].children, place(local(*k, 7, 5), 5));
    }
    let mut seen = vec![false; lowers.len()];
    for (n, o) in leaves.iter().enumerate() {
        let l = lower_no[&origin(*o, TileLevel::Upper)];
        if !seen[l] {
            seen[l] = true;
            lowers[l].first_child = U32::new(to_u32(n)?);
        }
        set(&mut lowers[l].children, place(local(*o, 3, 4), 4));
    }
    Ok(())
}

/// The root table: every upper node's key and every root tile's, sorted by key.
fn roots(
    upper_keys: &[Coord],
    tiles: &[(TileLevel, Coord)],
) -> Result<Vec<RootEntry>, FreezeError> {
    let mut by_key: BTreeMap<Coord, (u32, u32)> = BTreeMap::new();
    for (i, k) in upper_keys.iter().enumerate() {
        by_key.insert(*k, (ROOT_NODE, to_u32(i)?));
    }
    let root_tiles = tiles.iter().filter(|(l, _)| *l == TileLevel::Root).map(|(_, o)| *o);
    for o in root_tiles {
        by_key.insert(o, (ROOT_TILE, 0));
    }
    // Root tiles come first among tile values, numbered in table order.
    let mut root_tiles = 0_u32..;
    let entry = |(key, (kind, index)): (Coord, (u32, u32)),
                 tiles: &mut std::ops::RangeFrom<u32>| {
        let index = if kind == ROOT_TILE { tiles.next().unwrap_or(index) } else { index };
        RootEntry {
            key: key.map(I32::new),
            kind: U32::new(kind),
            index: U32::new(index),
            pad: [U32::ZERO; 3],
        }
    };
    Ok(by_key.into_iter().map(|e| entry(e, &mut root_tiles)).collect())
}

/// The order of tile values: root tiles in table order, then each upper node's, then each
/// lower node's, each node's in place order; marks every tile in its node.
fn tile_order(
    t: &Topology,
    tiles: &[(TileLevel, Coord)],
    uppers: &mut [UpperNode],
    lowers: &mut [LowerNode],
    upper_no: &BTreeMap<Coord, usize>,
    lower_no: &BTreeMap<Coord, usize>,
) -> Result<Vec<u32>, FreezeError> {
    let number = |o: Coord| match t.locate(o) {
        Location::Tile(n, _) => Ok(n.get()),
        Location::Leaf(_) | Location::Background => Err(FreezeError::Inconsistent),
    };
    let mut root: Vec<Coord> =
        tiles.iter().filter(|(l, _)| *l == TileLevel::Root).map(|(_, o)| *o).collect();
    root.sort_unstable();
    let mut order: Vec<u32> = root.iter().map(|o| number(*o)).collect::<Result<_, _>>()?;
    for level in [TileLevel::Upper, TileLevel::Lower] {
        let mut here: Vec<(usize, usize, Coord)> = Vec::new();
        for (_, o) in tiles.iter().filter(|(l, _)| *l == level) {
            let (node, p) = match level {
                TileLevel::Upper => {
                    (upper_no[&origin(*o, TileLevel::Root)], place(local(*o, 7, 5), 5))
                }
                TileLevel::Lower | TileLevel::Root => {
                    (lower_no[&origin(*o, TileLevel::Upper)], place(local(*o, 3, 4), 4))
                }
            };
            here.push((node, p, *o));
        }
        here.sort_unstable_by_key(|(node, p, _)| (*node, *p));
        let mut last = usize::MAX;
        for (node, p, o) in here {
            if node != last {
                last = node;
                let first = U32::new(to_u32(order.len())?);
                match level {
                    TileLevel::Upper => uppers[node].first_tile = first,
                    TileLevel::Lower | TileLevel::Root => lowers[node].first_tile = first,
                }
            }
            match level {
                TileLevel::Upper => set(&mut uppers[node].tiles, p),
                TileLevel::Lower | TileLevel::Root => set(&mut lowers[node].tiles, p),
            }
            order.push(number(o)?);
        }
    }
    Ok(order)
}

/// Blocks needed for `bytes` bytes.
fn blocks(bytes: usize) -> usize {
    bytes.div_ceil(BLOCK)
}

/// Packs a topology and a field on it into a frozen grid (ARCHITECTURE.md §4.6). The bytes
/// depend only on the topology and the values.
pub fn freeze<T: Value>(t: &Topology, f: &Field<T>) -> Result<FrozenGrid, FreezeError> {
    f.belongs_to(t)?;
    let plan = plan(t)?;
    let leaf_bytes = f.leaves(t)?.as_bytes();
    let tile_values: Vec<T> =
        plan.tiles.iter().map(|n| f.tile_value_at(*n)).collect::<Result<_, FieldError>>()?;
    let sizes = [
        plan.roots.as_bytes().len(),
        plan.uppers.as_bytes().len(),
        plan.lowers.as_bytes().len(),
        leaf_bytes.len(),
        tile_values.as_bytes().len(),
    ];
    let mut offsets = [0_usize; 5];
    let mut at = blocks(size_of::<Header>());
    for (o, s) in offsets.iter_mut().zip(sizes) {
        *o = at;
        at += blocks(s);
    }
    let mut buf = vec![Block([0; BLOCK]); at];
    let bytes = buf.as_mut_bytes();
    let header = header::<T>(&plan, f, &offsets, at, [t.leaf_count(), tile_values.len()])?;
    bytes[..size_of::<Header>()].copy_from_slice(header.as_bytes());
    let sections: [&[u8]; 5] = [
        plan.roots.as_bytes(),
        plan.uppers.as_bytes(),
        plan.lowers.as_bytes(),
        leaf_bytes,
        tile_values.as_bytes(),
    ];
    for (o, s) in offsets.iter().zip(sections) {
        bytes[o * BLOCK..o * BLOCK + s.len()].copy_from_slice(s);
    }
    Ok(FrozenGrid { blocks: buf })
}

fn header<T: Value>(
    plan: &Plan,
    f: &Field<T>,
    offsets: &[usize; 5],
    total: usize,
    [leaves, tiles]: [usize; 2],
) -> Result<Header, FreezeError> {
    let counts = [plan.roots.len(), plan.uppers.len(), plan.lowers.len(), leaves, tiles];
    let mut background = [U32::ZERO; 6];
    background
        .as_mut_bytes()
        .get_mut(..size_of::<T>())
        .ok_or(FreezeError::TooLarge)?
        .copy_from_slice(f.background().as_bytes());
    Ok(Header {
        magic: U32::new(MAGIC),
        version: U32::new(VERSION),
        value_type: U32::new(T::TYPE_CODE),
        value_size: U32::new(to_u32(size_of::<T>())?),
        counts: [
            to_u32(counts[0])?,
            to_u32(counts[1])?,
            to_u32(counts[2])?,
            to_u32(counts[3])?,
            to_u32(counts[4])?,
        ]
        .map(U32::new),
        offsets: [
            to_u32(offsets[0])?,
            to_u32(offsets[1])?,
            to_u32(offsets[2])?,
            to_u32(offsets[3])?,
            to_u32(offsets[4])?,
        ]
        .map(U32::new),
        blocks: U32::new(to_u32(total)?),
        background,
        pad: [U32::ZERO; 3],
    })
}
