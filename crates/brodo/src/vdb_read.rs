//! Reading OpenVDB files (ARCHITECTURE.md §4.7, ADR-0047): the float and double grids of a
//! `.vdb` file at file version 224 or 225, as brodo and OpenVDB 12 to 14 write them. This
//! module parses untrusted bytes: it is listed in `UNTRUSTED_INPUT`, indexes nothing
//! (CODING.md Rule 3), and is fuzzed (TESTING.md T6).
//!
//! The bytes are in `docs/formats.md` §9, which this code follows, §9.7 and §9.8 for what a
//! reader takes beyond what brodo writes. What it makes is counted against its limit before
//! it is made, each leaf, tile and node at what it holds at most (ARCHITECTURE.md §4.7).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::cache::read_within;
use crate::error::VdbError;
use crate::field::local_index;
use crate::field::{Field, LEAF_VOXELS};
use crate::meta::GridMeta;
use crate::topology::Topology;
use crate::tree::{Coord, TileLevel};
use crate::tree::{LEAF_LOG2, LOWER_DIM_LOG2, LOWER_SPAN_LOG2, UPPER_DIM_LOG2, UPPER_SPAN_LOG2};
use crate::vdb::{ACTIVE_MASK, MAGIC, VdbValue};

/// The file versions read (ADR-0047).
const VERSIONS: [u32; 2] = [224, 225];
/// Compression flags besides active-mask (docs/formats.md §9.7).
const ZIP: u32 = 1;
const BLOSC: u32 = 4;
/// Blosc's flag bits (docs/formats.md §9.8).
const BLOSC_SHUFFLE: u8 = 1;
const BLOSC_MEMCPY: u8 = 2;
const BLOSC_BITSHUFFLE: u8 = 4;
const BLOSC_NO_SPLIT: u8 = 0x10;
const BLOSC_LZ4: u8 = 1;
/// The places of an upper node and a lower node, as log₂ of a side, and of the edge of
/// their children: brodo's tree's, which OpenVDB's `5_4_3` trees share (docs/formats.md §9).
const UPPER_LOG: u32 = UPPER_DIM_LOG2;
const LOWER_LOG: u32 = LOWER_DIM_LOG2;
const UPPER_CHILD: u32 = LOWER_SPAN_LOG2;
const LOWER_CHILD: u32 = LEAF_LOG2;
/// The edge of the root's tiles and upper nodes.
const ROOT_EDGE: i32 = 1 << UPPER_SPAN_LOG2;
/// What a tile kept, a lower node and an upper node are counted to make (ARCHITECTURE.md
/// §4.7): a node's two masks twice, in the editor's tree and the one it commits, and room
/// for the rest; a tile at the most measured, about 160 bytes, rounded up (2026-11-03).
const TILE: u64 = 256;
const LOWER_NODE: u64 = 2 * 1024 + 512;
const UPPER_NODE: u64 = 2 * 8192 + 2048;

/// A grid's values, of the type its file states.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum VdbField {
    /// A grid of `Tree_float_5_4_3`.
    F32(Field<f32>),
    /// A grid of `Tree_double_5_4_3`.
    F64(Field<f64>),
}

/// One grid read from an OpenVDB file (docs/formats.md §9.7).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct VdbGrid {
    /// Its name, transform and voxel size, the unit the reader was given, and its string
    /// metadata as free pairs.
    pub meta: GridMeta,
    /// Its leaves, and its tiles active or not holding the background.
    pub topology: Topology,
    /// Its values on the topology, every leaf voxel's whether active or not.
    pub field: VdbField,
}

/// The grids of an OpenVDB file, in its order, and the names of those skipped.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct VdbFile {
    /// The float and double grids read.
    pub grids: Vec<VdbGrid>,
    /// The names of the grids skipped: of another type, saved as half floats, or instanced.
    pub skipped: Vec<String>,
}

/// Reads an OpenVDB file's bytes whole, as docs/formats.md §9.7 states, refusing a file
/// that breaks it; of several broken rules, which is named is not promised. OpenVDB states
/// no unit, so `unit` is every grid's. The leaves, tiles and nodes it makes are counted as
/// ARCHITECTURE.md §4.7 states, and more than `limit` bytes is refused before it is made.
///
/// A grid's string metadata is kept in its free pairs as the file holds it, so a key
/// beginning `brodo.` there, but `SDF_KEY`, makes `CacheEntry::new` refuse the grid with
/// `ReservedKey` (docs/formats.md §3.3): remove such a key before writing the grid to a
/// cache file, or write it with `CacheEntry::with_reserved_keys` to keep it.
pub fn from_bytes(bytes: &[u8], unit: &str, limit: u64) -> Result<VdbFile, VdbError> {
    let mut budget = Budget { left: limit, limit };
    let mut c = Cursor { bytes, at: 0 };
    let count = header(&mut c)?;
    let mut file = VdbFile { grids: Vec::new(), skipped: Vec::new() };
    for _ in 0..count {
        let d = descriptor(&mut c)?;
        let read = match d.kind.as_slice() {
            b"Tree_float_5_4_3" if d.instance.is_empty() => {
                let (meta, topology, f) = grid::<f32>(bytes, &d, unit, &mut budget)?;
                Some(VdbGrid { meta, topology, field: VdbField::F32(f) })
            }
            b"Tree_double_5_4_3" if d.instance.is_empty() => {
                let (meta, topology, f) = grid::<f64>(bytes, &d, unit, &mut budget)?;
                Some(VdbGrid { meta, topology, field: VdbField::F64(f) })
            }
            _ => None,
        };
        match read {
            Some(g) => file.grids.push(g),
            None => file.skipped.push(d.name),
        }
        c.at = d.end;
    }
    if c.at != bytes.len() {
        return Err(VdbError::Malformed("file's end"));
    }
    Ok(file)
}

/// Reads an OpenVDB file of at most `limit` bytes, making at most `limit` bytes more, as
/// `from_bytes` does: a longer file is refused by its length before any of it is read, or by
/// the bytes read where its length says less than it holds.
pub fn read(path: &Path, unit: &str, limit: u64) -> Result<VdbFile, VdbError> {
    let bytes = read_within(path, limit, |length, limit| VdbError::FileTooLarge { length, limit })?;
    from_bytes(&bytes, unit, limit)
}

/// A value type read: its bytes, its negation and its bits.
trait Scalar: VdbValue + std::ops::Neg<Output = Self> {
    const SIZE: usize;
    /// What a leaf is counted to make (ARCHITECTURE.md §4.7): its values twice, in the
    /// reader's map and the field, and 1024 bytes for its mask, origin and numbers.
    const LEAF: u64;

    fn get(bytes: &[u8]) -> Option<Self>;

    /// Whether two values have the same bits.
    fn same(self, other: Self) -> bool;
}

impl Scalar for f32 {
    const SIZE: usize = 4;
    const LEAF: u64 = 2 * 512 * 4 + 1024;

    fn get(bytes: &[u8]) -> Option<Self> {
        Some(Self::from_le_bytes(bytes.try_into().ok()?))
    }

    fn same(self, other: Self) -> bool {
        self.to_bits() == other.to_bits()
    }
}

impl Scalar for f64 {
    const SIZE: usize = 8;
    const LEAF: u64 = 2 * 512 * 8 + 1024;

    fn get(bytes: &[u8]) -> Option<Self> {
        Some(Self::from_le_bytes(bytes.try_into().ok()?))
    }

    fn same(self, other: Self) -> bool {
        self.to_bits() == other.to_bits()
    }
}

/// What reading may still make of its limit.
struct Budget {
    left: u64,
    limit: u64,
}

impl Budget {
    /// `n` bytes more, refused if they pass the limit.
    fn take(&mut self, n: u64) -> Result<(), VdbError> {
        let length = (self.limit - self.left).saturating_add(n);
        self.left =
            self.left.checked_sub(n).ok_or(VdbError::FileTooLarge { length, limit: self.limit })?;
        Ok(())
    }
}

/// The bytes of a file, read from `at` on.
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize, what: &'static str) -> Result<&'a [u8], VdbError> {
        let end = self.at.checked_add(n).ok_or(VdbError::Malformed(what))?;
        let out = self.bytes.get(self.at..end).ok_or(VdbError::Malformed(what))?;
        self.at = end;
        Ok(out)
    }

    fn array<const N: usize>(&mut self, what: &'static str) -> Result<[u8; N], VdbError> {
        self.take(N, what)?.try_into().map_err(|_| VdbError::Malformed(what))
    }

    fn u8(&mut self, what: &'static str) -> Result<u8, VdbError> {
        Ok(u8::from_le_bytes(self.array(what)?))
    }

    fn u32(&mut self, what: &'static str) -> Result<u32, VdbError> {
        Ok(u32::from_le_bytes(self.array(what)?))
    }

    fn i32(&mut self, what: &'static str) -> Result<i32, VdbError> {
        Ok(i32::from_le_bytes(self.array(what)?))
    }

    fn i64(&mut self, what: &'static str) -> Result<i64, VdbError> {
        Ok(i64::from_le_bytes(self.array(what)?))
    }

    fn f64(&mut self, what: &'static str) -> Result<f64, VdbError> {
        Ok(f64::from_le_bytes(self.array(what)?))
    }

    fn value<T: Scalar>(&mut self, what: &'static str) -> Result<T, VdbError> {
        T::get(self.take(T::SIZE, what)?).ok_or(VdbError::Malformed(what))
    }

    /// A string's bytes: a `u32` length and that many bytes.
    fn string(&mut self, what: &'static str) -> Result<&'a [u8], VdbError> {
        let len = usize::try_from(self.u32(what)?).map_err(|_| VdbError::Malformed(what))?;
        self.take(len, what)
    }

    /// A mask of `places` places, as `u64` words.
    fn mask(&mut self, places: usize, what: &'static str) -> Result<Vec<u64>, VdbError> {
        let bytes = self.take(places / 8, what)?;
        Ok(bytes.as_chunks::<8>().0.iter().map(|w| u64::from_le_bytes(*w)).collect())
    }
}

/// Whether place `p` of a mask is set.
fn bit(mask: &[u64], p: usize) -> bool {
    mask.get(p / 64).is_some_and(|w| w >> (p % 64) & 1 == 1)
}

fn utf8(bytes: &[u8], what: &'static str) -> Result<String, VdbError> {
    String::from_utf8(bytes.to_vec()).map_err(|_| VdbError::Malformed(what))
}

/// The header (docs/formats.md §9.1, §9.7 rule 1), and the grid count.
fn header(c: &mut Cursor<'_>) -> Result<usize, VdbError> {
    if c.i64("magic")? != MAGIC {
        return Err(VdbError::Malformed("magic"));
    }
    let version = c.u32("file version")?;
    if !VERSIONS.contains(&version) {
        return Err(VdbError::Version(version));
    }
    c.take(8, "library version")?;
    if c.u8("grid offsets")? != 1 {
        return Err(VdbError::Unsupported("a file without grid offsets"));
    }
    c.take(36, "UUID")?;
    for _ in 0..c.u32("file metadata")? {
        c.string("file metadata")?;
        c.string("file metadata")?;
        c.string("file metadata")?;
    }
    usize::try_from(c.i32("grid count")?).map_err(|_| VdbError::Malformed("grid count"))
}

/// A grid descriptor (docs/formats.md §9.2): its name, type and instance string, and its
/// body's three offsets, its start the descriptor's end and its end within the file.
struct Descriptor {
    name: String,
    kind: Vec<u8>,
    instance: Vec<u8>,
    start: usize,
    buffers: usize,
    end: usize,
}

fn descriptor(c: &mut Cursor<'_>) -> Result<Descriptor, VdbError> {
    let name = utf8(c.string("grid name")?, "grid name")?;
    let kind = c.string("grid type")?.to_vec();
    let instance = c.string("instance")?.to_vec();
    let mut offsets = [0_usize; 3];
    for o in &mut offsets {
        *o = usize::try_from(c.i64("grid offset")?)
            .map_err(|_| VdbError::Malformed("grid offset"))?;
    }
    // An instanced grid states its leaf buffers' offset as 0; it is skipped, unread.
    let [start, buffers, end] = offsets;
    if start != c.at || end < start || end > c.bytes.len() {
        return Err(VdbError::Malformed("grid offset"));
    }
    Ok(Descriptor { name, kind, instance, start, buffers, end })
}

/// A grid's leaves and tiles as read: each leaf's values by origin, in brodo's order, and
/// each tile kept, by level and origin.
struct Tree<T> {
    leaves: BTreeMap<Coord, Box<[T; LEAF_VOXELS]>>,
    tiles: BTreeMap<(TileLevel, Coord), T>,
}

/// A grid's body.
fn grid<T: Scalar>(
    bytes: &[u8],
    d: &Descriptor,
    unit: &str,
    budget: &mut Budget,
) -> Result<(GridMeta, Topology, Field<T>), VdbError> {
    let mut c = Cursor { bytes, at: d.start };
    if d.buffers < d.start || d.end < d.buffers {
        return Err(VdbError::Malformed("grid offset"));
    }
    let flags = c.u32("compression flags")?;
    if flags & ZIP != 0 {
        return Err(VdbError::Unsupported("zip compression"));
    }
    if flags & !(ACTIVE_MASK | BLOSC) != 0 {
        return Err(VdbError::Malformed("compression flags"));
    }
    let extra = metadata(&mut c)?;
    let transform = transform(&mut c)?;
    let mut meta = GridMeta::new(&d.name, voxel_size(&transform), unit)?;
    meta.transform = transform;
    meta.extra = extra;
    meta.check()?;
    let (background, mut tree, leaves) = topology::<T>(&mut c, flags, budget)?;
    if c.at != d.buffers {
        return Err(VdbError::Malformed("topology's end"));
    }
    for (origin, mask) in leaves {
        if c.mask(512, "leaf mask")? != mask {
            return Err(VdbError::Malformed("leaf mask"));
        }
        let values = values::<T>(&mut c, 512, &mask, flags, background)?;
        tree.leaves.insert(origin, brodo_order(&values)?);
    }
    if c.at != d.end {
        return Err(VdbError::Malformed("leaf buffers' end"));
    }
    let (t, f) = build(&tree, background)?;
    Ok((meta, t, f))
}

/// A leaf's values from OpenVDB's place order, z fastest, to brodo's, x fastest.
fn brodo_order<T: Scalar>(values: &[T]) -> Result<Box<[T; LEAF_VOXELS]>, VdbError> {
    let mut out =
        Box::new([values.first().copied().ok_or(VdbError::Malformed("leaf"))?; LEAF_VOXELS]);
    for (p, v) in values.iter().enumerate() {
        // Each of `p`'s three octal digits is below 8, so in `i32`.
        let [x, y, z] = [p >> 6, (p >> 3) & 7, p & 7].map(|c| i32::try_from(c).unwrap_or(0));
        *out.get_mut(local_index([x, y, z])).ok_or(VdbError::Malformed("leaf"))? = *v;
    }
    Ok(out)
}

/// The topology and field of the leaves and tiles read.
fn build<T: Scalar>(tree: &Tree<T>, background: T) -> Result<(Topology, Field<T>), VdbError> {
    let empty = Topology::new();
    let mut e = empty.edit();
    for origin in tree.leaves.keys() {
        e.activate(*origin)?;
    }
    for (level, origin) in tree.tiles.keys() {
        e.add_tile(*level, *origin)?;
    }
    let (t, _) = e.commit()?;
    let mut f = Field::new(&t, background);
    for n in t.leaf_numbers() {
        let values = tree.leaves.get(&t.leaf_origin(n)?).ok_or(VdbError::Malformed("leaf"))?;
        f.leaf_mut(&t, n)?.copy_from_slice(values.as_slice());
    }
    for n in t.tile_numbers() {
        let value = tree.tiles.get(&t.tile(n)?).ok_or(VdbError::Malformed("tile"))?;
        f.set_tile(&t, n, *value)?;
    }
    Ok((t, f))
}

/// A grid's free pairs.
type Pairs = BTreeMap<String, String>;

/// The grid's metadata (docs/formats.md §9.7 rule 4): its string entries kept, but `name`
/// and those beginning `file_`.
fn metadata(c: &mut Cursor<'_>) -> Result<Pairs, VdbError> {
    let mut extra = BTreeMap::new();
    for _ in 0..c.u32("metadata count")? {
        let key = c.string("metadata name")?;
        let kind = c.string("metadata type")?;
        let value = c.string("metadata value")?;
        if kind == b"string" && key != b"name" && !key.starts_with(b"file_") {
            extra.insert(utf8(key, "metadata name")?, utf8(value, "metadata value")?);
        }
    }
    Ok(extra)
}

/// The transform (docs/formats.md §9.7 rule 5) as brodo's 3×4 map.
fn transform(c: &mut Cursor<'_>) -> Result<[[f64; 4]; 3], VdbError> {
    let kind = c.string("map")?;
    let mut f = |n: usize| (0..n).map(|_| c.f64("map")).collect::<Result<Vec<f64>, VdbError>>();
    let m = match kind {
        b"AffineMap" => {
            let v = f(16)?;
            let row = |r: usize| -> [f64; 4] {
                let mut out = [0.0; 4];
                for (o, x) in out.iter_mut().zip(v.iter().skip(4 * r)) {
                    *o = *x;
                }
                out
            };
            let m = [row(0), row(1), row(2), row(3)];
            let [[_, _, _, a], [_, _, _, b], [_, _, _, d], [_, _, _, one]] = m;
            if [a, b, d].iter().any(|x| x.abs().to_bits() != 0)
                || one.to_bits() != 1.0_f64.to_bits()
            {
                return Err(VdbError::Malformed("AffineMap"));
            }
            m
        }
        b"ScaleMap" | b"UniformScaleMap" => scaled(&f(15)?, [0.0; 3]),
        b"UniformScaleTranslateMap" => {
            let v = f(18)?;
            let t = three(&v, 0);
            scaled(v.get(3..).unwrap_or_default(), t)
        }
        _ => return Err(VdbError::Map(String::from_utf8_lossy(kind).into_owned())),
    };
    let [[a, b, cc, _], [d, e, g, _], [h, i, j, _], [x, y, z, _]] = m;
    let row = |p: f64, q: f64, r: f64, t: f64| [p, q, r, t - 0.5 * (p + q + r)];
    Ok([row(a, d, h, x), row(b, e, i, y), row(cc, g, j, z)])
}

fn three(v: &[f64], from: usize) -> [f64; 3] {
    let mut out = [0.0; 3];
    for (o, x) in out.iter_mut().zip(v.iter().skip(from)) {
        *o = *x;
    }
    out
}

/// OpenVDB's matrix of a scale, the first three of `fields`, and a translation `t`.
fn scaled(fields: &[f64], t: [f64; 3]) -> [[f64; 4]; 4] {
    let [sx, sy, sz] = three(fields, 0);
    let [tx, ty, tz] = t;
    [[sx, 0.0, 0.0, 0.0], [0.0, sy, 0.0, 0.0], [0.0, 0.0, sz, 0.0], [tx, ty, tz, 1.0]]
}

/// The length of each of a map's first three columns.
fn voxel_size(a: &[[f64; 4]; 3]) -> [f64; 3] {
    let [[a0, a1, a2, _], [b0, b1, b2, _], [c0, c1, c2, _]] = *a;
    let len = |x: f64, y: f64, z: f64| (x * x + y * y + z * z).sqrt();
    [len(a0, b0, c0), len(a1, b1, c1), len(a2, b2, c2)]
}

/// A leaf's origin and value mask, from the topology.
type LeafMask = (Coord, Vec<u64>);

/// The topology (docs/formats.md §9.4, §9.7 rule 6): the background, the tiles kept, and
/// each leaf's origin and value mask in the file's order.
fn topology<T: Scalar>(
    c: &mut Cursor<'_>,
    flags: u32,
    budget: &mut Budget,
) -> Result<(T, Tree<T>, Vec<LeafMask>), VdbError> {
    if c.i32("buffer count")? != 1 {
        return Err(VdbError::Malformed("buffer count"));
    }
    let background = c.value::<T>("background")?;
    let mut tree = Tree { leaves: BTreeMap::new(), tiles: BTreeMap::new() };
    let mut leaves = Vec::new();
    let (tiles, uppers) = (c.u32("root's tile count")?, c.u32("root's node count")?);
    let mut origins = BTreeSet::new();
    for _ in 0..tiles {
        let o = root_origin(c, &mut origins)?;
        let value = c.value::<T>("root tile")?;
        let active = match c.u8("root tile")? {
            0 => false,
            1 => true,
            _ => return Err(VdbError::Malformed("root tile")),
        };
        if active || !value.same(background) {
            budget.take(TILE)?;
            tree.tiles.insert((TileLevel::Root, o), value);
        }
    }
    let node = Node { flags, background };
    for _ in 0..uppers {
        let o = root_origin(c, &mut origins)?;
        budget.take(UPPER_NODE)?;
        node.upper(c, o, &mut tree, &mut leaves, budget)?;
    }
    Ok((background, tree, leaves))
}

/// An origin at the root, a multiple of its edge, refused if seen before.
fn root_origin(c: &mut Cursor<'_>, seen: &mut BTreeSet<Coord>) -> Result<Coord, VdbError> {
    let o = [c.i32("origin")?, c.i32("origin")?, c.i32("origin")?];
    if o.iter().any(|x| x % ROOT_EDGE != 0) || !seen.insert(o) {
        return Err(VdbError::Malformed("root's origin"));
    }
    Ok(o)
}

/// A node's level: log₂ of its side in places, of a child's edge, and its tiles' level.
#[derive(Clone, Copy)]
struct Level {
    log: u32,
    child: u32,
    level: TileLevel,
}

/// What every node of a grid reads its values with.
struct Node<T> {
    flags: u32,
    background: T,
}

impl<T: Scalar> Node<T> {
    /// An upper node at `origin`, then its lower nodes.
    fn upper(
        &self,
        c: &mut Cursor<'_>,
        origin: Coord,
        tree: &mut Tree<T>,
        leaves: &mut Vec<LeafMask>,
        budget: &mut Budget,
    ) -> Result<(), VdbError> {
        let upper = Level { log: UPPER_LOG, child: UPPER_CHILD, level: TileLevel::Upper };
        let lower = Level { log: LOWER_LOG, child: LOWER_CHILD, level: TileLevel::Lower };
        for o in self.places(c, origin, &upper, LOWER_NODE, tree, budget)? {
            for lo in self.places(c, o, &lower, T::LEAF, tree, budget)? {
                leaves.push((lo, c.mask(512, "leaf mask")?));
            }
        }
        Ok(())
    }

    /// A node's masks and values (docs/formats.md §9.5): its tiles kept into `tree`, and its
    /// children's origins returned in place order, each child counted at `charge`.
    fn places(
        &self,
        c: &mut Cursor<'_>,
        origin: Coord,
        at: &Level,
        charge: u64,
        tree: &mut Tree<T>,
        budget: &mut Budget,
    ) -> Result<Vec<Coord>, VdbError> {
        let Level { log, child, level } = *at;
        let n = 1_usize << (3 * log);
        let children = c.mask(n, "child mask")?;
        let active = c.mask(n, "value mask")?;
        let values = values::<T>(c, n, &active, self.flags, self.background)?;
        let mut out = Vec::new();
        for (p, v) in values.into_iter().enumerate() {
            let o = place_origin(origin, p, log, child)?;
            if bit(&children, p) {
                budget.take(charge)?;
                out.push(o);
            } else if bit(&active, p) || !v.same(self.background) {
                budget.take(TILE)?;
                tree.tiles.insert((level, o), v);
            }
        }
        Ok(out)
    }
}

/// The origin of place `p` of a node at `origin`, z fastest (docs/formats.md §9.4).
fn place_origin(origin: Coord, p: usize, log: u32, child: u32) -> Result<Coord, VdbError> {
    let side = (1_usize << log) - 1;
    let local = [p >> (2 * log), (p >> log) & side, p & side];
    let mut out = origin;
    for (o, l) in out.iter_mut().zip(local) {
        let step = i32::try_from(l << child).map_err(|_| VdbError::Malformed("origin"))?;
        *o = o.checked_add(step).ok_or(VdbError::Malformed("origin"))?;
    }
    Ok(out)
}

/// A node's `places` values (docs/formats.md §9.8), `mask` its active places.
fn values<T: Scalar>(
    c: &mut Cursor<'_>,
    places: usize,
    mask: &[u64],
    flags: u32,
    background: T,
) -> Result<Vec<T>, VdbError> {
    let indicator = c.u8("indicator")?;
    let inactive = match indicator {
        0 | 1 | 3 | 6 => 0,
        2 | 4 => 1,
        5 => 2,
        _ => return Err(VdbError::Malformed("indicator")),
    };
    let stored_inactive = (0..inactive)
        .map(|_| c.value::<T>("inactive value"))
        .collect::<Result<Vec<T>, VdbError>>()?;
    let select =
        if (3..=5).contains(&indicator) { c.mask(places, "selection mask")? } else { Vec::new() };
    let all = indicator == 6 || flags & ACTIVE_MASK == 0;
    let active: u32 = mask.iter().map(|w| w.count_ones()).sum();
    let count = if all {
        places
    } else {
        usize::try_from(active).map_err(|_| VdbError::Malformed("values"))?
    };
    let raw = count.checked_mul(T::SIZE).ok_or(VdbError::Malformed("values"))?;
    let data = stored(c, raw, flags)?;
    let mut stored = (0..count).map(|k| data.get(k * T::SIZE..(k + 1) * T::SIZE).and_then(T::get));
    let first = stored_inactive.first().copied().unwrap_or(background);
    let second = stored_inactive.get(1).copied().unwrap_or(background);
    let mut out = Vec::with_capacity(places);
    for p in 0..places {
        let v = if all || bit(mask, p) {
            stored.next().flatten().ok_or(VdbError::Malformed("values"))?
        } else {
            let s = bit(&select, p);
            match indicator {
                0 => background,
                1 => -background,
                2 => first,
                3 if s => background,
                3 => -background,
                4 if s => background,
                4 => first,
                5 if s => second,
                _ => first,
            }
        };
        out.push(v);
    }
    Ok(out)
}

/// The `raw` bytes of a node's values stored (docs/formats.md §9.8): as they are, or with
/// Blosc, after their `i64` count, a chunk or as they are.
fn stored(c: &mut Cursor<'_>, raw: usize, flags: u32) -> Result<Vec<u8>, VdbError> {
    if flags & BLOSC == 0 {
        return Ok(c.take(raw, "values")?.to_vec());
    }
    let n = c.i64("values' byte count")?;
    if n > 0 {
        let len = usize::try_from(n).map_err(|_| VdbError::Malformed("values' byte count"))?;
        return blosc(c.take(len, "Blosc chunk")?, raw);
    }
    if n.checked_neg().and_then(|m| usize::try_from(m).ok()) != Some(raw) {
        return Err(VdbError::Malformed("values' byte count"));
    }
    Ok(c.take(raw, "values")?.to_vec())
}

/// A Blosc chunk of `raw` bytes decoded (docs/formats.md §9.8).
fn blosc(chunk: &[u8], raw: usize) -> Result<Vec<u8>, VdbError> {
    let mut c = Cursor { bytes: chunk, at: 0 };
    let (version, _, flags, size) =
        (c.u8("Blosc")?, c.u8("Blosc")?, c.u8("Blosc")?, c.u8("Blosc")?);
    let mut word = || c.u32("Blosc").map(|w| usize::try_from(w).unwrap_or(usize::MAX));
    let (nbytes, block, cbytes) = (word()?, word()?, word()?);
    if version != 2 {
        return Err(VdbError::Unsupported("a Blosc chunk of another format version"));
    }
    if cbytes != chunk.len() || nbytes != raw {
        return Err(VdbError::Malformed("Blosc chunk"));
    }
    if flags & BLOSC_MEMCPY != 0 {
        let out = c.take(nbytes, "Blosc chunk")?;
        return if c.at == chunk.len() {
            Ok(out.to_vec())
        } else {
            Err(VdbError::Malformed("Blosc chunk"))
        };
    }
    if flags >> 5 != BLOSC_LZ4 || flags & BLOSC_BITSHUFFLE != 0 {
        return Err(VdbError::Unsupported("a Blosc codec other than LZ4 with byte shuffle"));
    }
    if nbytes == 0 {
        return Err(VdbError::Malformed("Blosc chunk"));
    }
    if nbytes > block {
        return Err(VdbError::Unsupported("a Blosc chunk of more than one block"));
    }
    let start = c.i32("Blosc block start")?;
    if usize::try_from(start).ok() != Some(c.at) {
        return Err(VdbError::Malformed("Blosc chunk"));
    }
    let splits = if flags & BLOSC_NO_SPLIT == 0 && nbytes == block { usize::from(size) } else { 1 };
    if splits == 0 || nbytes % splits != 0 {
        return Err(VdbError::Malformed("Blosc chunk"));
    }
    let out = streams(&mut c, splits, nbytes / splits)?;
    if c.at != chunk.len() {
        return Err(VdbError::Malformed("Blosc chunk"));
    }
    Ok(if flags & BLOSC_SHUFFLE != 0 { unshuffle(&out, usize::from(size)) } else { out })
}

/// A block's `splits` streams of `part` bytes each, each an `i32` length and that many
/// bytes, LZ4's block format or, of the stream's own length, as it is.
fn streams(c: &mut Cursor<'_>, splits: usize, part: usize) -> Result<Vec<u8>, VdbError> {
    let mut out = Vec::with_capacity(splits.saturating_mul(part));
    for _ in 0..splits {
        let len = usize::try_from(c.i32("Blosc split")?)
            .map_err(|_| VdbError::Malformed("Blosc split"))?;
        let bytes = c.take(len, "Blosc split")?;
        if len == part {
            out.extend_from_slice(bytes);
        } else {
            let mut buf = vec![0_u8; part];
            if lz4_flex::block::decompress_into(bytes, &mut buf).ok() != Some(part) {
                return Err(VdbError::Malformed("LZ4 block"));
            }
            out.extend(buf);
        }
    }
    Ok(out)
}

/// A block byte-shuffled by `size` restored: of its m whole values, byte b of value j is
/// at b × m + j; a remainder past them stays in place.
fn unshuffle(block: &[u8], size: usize) -> Vec<u8> {
    let mut out = block.to_vec();
    let m = block.len().checked_div(size).unwrap_or(0);
    if m == 0 || size < 2 {
        return out;
    }
    for (b, stream) in block.chunks_exact(m).take(size).enumerate() {
        for (j, byte) in stream.iter().enumerate() {
            if let Some(o) = out.get_mut(j * size + b) {
                *o = *byte;
            }
        }
    }
    out
}
