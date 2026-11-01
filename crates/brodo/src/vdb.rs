//! OpenVDB files (ARCHITECTURE.md §4.7, ADR-0045, ADR-0046, ADR-0047): grids of `f32` or
//! `f64` written as one `.vdb` file at file version 225, as OpenVDB 14 reads it, and read
//! from the files brodo and OpenVDB 12 to 14 write (`from_bytes`, `read`).
//!
//! The bytes are in `docs/formats.md` §9, which this code follows. brodo's tree has
//! OpenVDB's shape, so each of brodo's nodes is one of OpenVDB's; what differs is order.
//! OpenVDB numbers a node's places z fastest and lists nodes by origin and place, where
//! brodo numbers a leaf's voxels x fastest and its leaves and tiles in Morton order; the
//! nodes are gathered by place first, in `BTreeMap`s, so the same grid always gives the
//! same bytes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::error::VdbError;
use crate::field::{Field, Stored};
use crate::meta::{Bounds, GridMeta, SDF_KEY};
use crate::topology::{LeafNumber, TileNumber, Topology};
use crate::tree::{Coord, TileLevel};
pub use crate::vdb_read::{VdbField, VdbFile, VdbGrid, from_bytes, read};

/// The file's magic number, an `i64`.
pub(crate) const MAGIC: i64 = 0x5644_4220;
/// The file version written (ADR-0046).
const FILE_VERSION: u32 = 225;
/// The library version a file states, major and minor (ADR-0046).
const LIBRARY: [u32; 2] = [14, 0];
/// The file's UUID: zeros, so that the same grids give the same bytes (ADR-0046).
const UUID: &[u8; 36] = b"00000000-0000-0000-0000-000000000000";
/// Compression flags: active-mask, the one brodo writes alone (ADR-0046).
pub(crate) const ACTIVE_MASK: u32 = 2;
/// The places of an upper node, a lower node and a leaf.
const UPPER_PLACES: usize = 1 << 15;
const LOWER_PLACES: usize = 1 << 12;
const LEAF_PLACES: usize = 1 << 9;
/// `file_bbox_min` and `file_bbox_max` of a grid with no leaf or tile, as OpenVDB 14.0.0
/// writes them for an empty grid (docs/formats.md §9.3).
const EMPTY_BOX: (Coord, Coord) = ([i32::MAX; 3], [i32::MIN; 3]);

/// A value type OpenVDB holds a grid of: `f32` or `f64` (ADR-0046).
pub trait VdbValue: Stored {
    /// OpenVDB's name for a tree of the type.
    const TREE_TYPE: &'static str;

    /// Appends the value's little-endian bytes.
    fn put(self, out: &mut Vec<u8>);
}

impl VdbValue for f32 {
    const TREE_TYPE: &'static str = "Tree_float_5_4_3";

    fn put(self, out: &mut Vec<u8>) {
        out.extend(self.to_le_bytes());
    }
}

impl VdbValue for f64 {
    const TREE_TYPE: &'static str = "Tree_double_5_4_3";

    fn put(self, out: &mut Vec<u8>) {
        out.extend(self.to_le_bytes());
    }
}

/// A grid ready to be written: its name, its tree type, and its body's bytes, the topology
/// and the leaf buffers apart (docs/formats.md §9.3).
#[derive(Debug, Clone)]
pub struct VdbEntry {
    name: String,
    tree_type: &'static str,
    body: Vec<u8>,
    buffers: Vec<u8>,
}

impl VdbEntry {
    /// A topology and a field on it, with its metadata, for an OpenVDB file.
    ///
    /// Of the metadata, the name and the transform are written, and `sdf.version` among
    /// the free pairs makes the grid a level set; the unit, the voxel size and the other
    /// pairs are not written, OpenVDB having no place for a unit and the transform alone
    /// placing the voxels. Every leaf voxel and tile is written active (ADR-0046).
    pub fn new<T: VdbValue>(meta: &GridMeta, t: &Topology, f: &Field<T>) -> Result<Self, VdbError> {
        meta.check()?;
        f.belongs_to(t)?;
        let nodes = Nodes::of(t)?;
        let mut body = Vec::new();
        put_u32(&mut body, ACTIVE_MASK);
        put_metadata(&mut body, meta, t)?;
        put_transform(&mut body, &meta.transform)?;
        put_topology(&mut body, &nodes, t, f)?;
        let mut buffers = Vec::new();
        for n in nodes.leaves() {
            put_mask(&mut buffers, LEAF_PLACES, 0..LEAF_PLACES);
            buffers.push(0);
            let values = f.leaf(t, n)?;
            // Place (x, y, z) is x·64 + y·8 + z; brodo's index is x + 8·y + 64·z.
            for x in 0..8 {
                for y in 0..8 {
                    for z in 0..8 {
                        values[x + 8 * y + 64 * z].put(&mut buffers);
                    }
                }
            }
        }
        Ok(Self { name: meta.name.clone(), tree_type: T::TREE_TYPE, body, buffers })
    }
}

/// The bytes of an OpenVDB file holding `entries`, in order. Two grids of one name are
/// refused.
pub fn to_bytes(entries: &[VdbEntry]) -> Result<Vec<u8>, VdbError> {
    let mut names = BTreeSet::new();
    if let Some(e) = entries.iter().find(|e| !names.insert(e.name.as_str())) {
        return Err(VdbError::DuplicateName(e.name.clone()));
    }
    let mut out = Vec::new();
    out.extend(MAGIC.to_le_bytes());
    for x in [FILE_VERSION, LIBRARY[0], LIBRARY[1]] {
        put_u32(&mut out, x);
    }
    out.push(1);
    out.extend(UUID);
    put_u32(&mut out, 0);
    let count = i32::try_from(entries.len()).map_err(|_| VdbError::TooLarge("grid count"))?;
    out.extend(count.to_le_bytes());
    for e in entries {
        put_str(&mut out, &e.name)?;
        put_str(&mut out, e.tree_type)?;
        put_str(&mut out, "")?;
        let body = out.len() + 24;
        let buffers = body + e.body.len();
        for at in [body, buffers, buffers + e.buffers.len()] {
            let at = i64::try_from(at).map_err(|_| VdbError::TooLarge("offset"))?;
            out.extend(at.to_le_bytes());
        }
        out.extend(&e.body);
        out.extend(&e.buffers);
    }
    Ok(out)
}

/// Writes an OpenVDB file holding `entries`, replacing a file at `path` whole as
/// `cache::write` does.
pub fn write(path: &Path, entries: &[VdbEntry]) -> Result<(), VdbError> {
    use std::io::Write;
    let bytes = to_bytes(entries)?;
    crate::cache::replace(path, |out| Ok::<(), VdbError>(out.write_all(&bytes)?))
}

/// A lower node's leaves and tiles, by place.
#[derive(Default)]
struct Lower {
    leaves: BTreeMap<usize, LeafNumber>,
    tiles: BTreeMap<usize, TileNumber>,
}

/// An upper node's lower nodes and tiles, by place.
#[derive(Default)]
struct Upper {
    lowers: BTreeMap<usize, Lower>,
    tiles: BTreeMap<usize, TileNumber>,
}

/// A topology as OpenVDB's nodes: the root's tiles and upper nodes, by origin.
struct Nodes {
    root_tiles: BTreeMap<Coord, TileNumber>,
    uppers: BTreeMap<Coord, Upper>,
}

impl Nodes {
    fn of(t: &Topology) -> Result<Self, VdbError> {
        let mut nodes = Self { root_tiles: BTreeMap::new(), uppers: BTreeMap::new() };
        for n in t.leaf_numbers() {
            let o = t.leaf_origin(n)?;
            nodes.lower(o).leaves.insert(place(o, 3, 4), n);
        }
        for n in t.tile_numbers() {
            let (level, o) = t.tile(n)?;
            match level {
                TileLevel::Root => {
                    nodes.root_tiles.insert(o, n);
                }
                TileLevel::Upper => {
                    nodes.upper(o).tiles.insert(place(o, 7, 5), n);
                }
                TileLevel::Lower => {
                    nodes.lower(o).tiles.insert(place(o, 3, 4), n);
                }
            }
        }
        Ok(nodes)
    }

    fn upper(&mut self, v: Coord) -> &mut Upper {
        self.uppers.entry(TileLevel::Root.origin(v)).or_default()
    }

    fn lower(&mut self, v: Coord) -> &mut Lower {
        self.upper(v).lowers.entry(place(v, 7, 5)).or_default()
    }

    /// The leaves in the order the file holds them.
    fn leaves(&self) -> impl Iterator<Item = LeafNumber> + '_ {
        self.uppers
            .values()
            .flat_map(|u| u.lowers.values())
            .flat_map(|l| l.leaves.values().copied())
    }
}

/// A voxel's place in a node whose children have edge 2^`child` and which is 2^`log`
/// places a side, z fastest (docs/formats.md §9.4).
fn place(v: Coord, child: u32, log: u32) -> usize {
    // Lossless: brodo's targets have pointers of at least 32 bits (`TARGETS`).
    let [x, y, z] = v.map(|c| ((c.cast_unsigned() >> child) & ((1 << log) - 1)) as usize);
    (x << (2 * log)) | (y << log) | z
}

/// A mask of `places` places as `u64` words, place p at bit p mod 64 of word p div 64.
fn put_mask(out: &mut Vec<u8>, places: usize, on: impl Iterator<Item = usize>) {
    let mut words = vec![0_u64; places / 64];
    for p in on {
        if let Some(w) = words.get_mut(p / 64) {
            *w |= 1 << (p % 64);
        }
    }
    for w in words {
        out.extend(w.to_le_bytes());
    }
}

/// A node's values (docs/formats.md §9.5): 0, its tiles' values alone, in place order.
fn put_values<T: VdbValue>(
    out: &mut Vec<u8>,
    tiles: &BTreeMap<usize, TileNumber>,
    t: &Topology,
    f: &Field<T>,
) -> Result<(), VdbError> {
    out.push(0);
    for n in tiles.values() {
        f.tile(t, *n)?.put(out);
    }
    Ok(())
}

/// The topology (docs/formats.md §9.4).
fn put_topology<T: VdbValue>(
    out: &mut Vec<u8>,
    nodes: &Nodes,
    t: &Topology,
    f: &Field<T>,
) -> Result<(), VdbError> {
    out.extend(1_i32.to_le_bytes());
    f.background().put(out);
    put_u32(out, u32_of(nodes.root_tiles.len(), "root's tile count")?);
    put_u32(out, u32_of(nodes.uppers.len(), "root's node count")?);
    for (o, n) in &nodes.root_tiles {
        put_coord(out, *o);
        f.tile(t, *n)?.put(out);
        out.push(1);
    }
    for (o, u) in &nodes.uppers {
        put_coord(out, *o);
        put_mask(out, UPPER_PLACES, u.lowers.keys().copied());
        put_mask(out, UPPER_PLACES, u.tiles.keys().copied());
        put_values(out, &u.tiles, t, f)?;
        for l in u.lowers.values() {
            put_mask(out, LOWER_PLACES, l.leaves.keys().copied());
            put_mask(out, LOWER_PLACES, l.tiles.keys().copied());
            put_values(out, &l.tiles, t, f)?;
            for _ in l.leaves.values() {
                put_mask(out, LEAF_PLACES, 0..LEAF_PLACES);
            }
        }
    }
    Ok(())
}

/// The grid's metadata entries, sorted by name (docs/formats.md §9.3).
fn put_metadata(out: &mut Vec<u8>, meta: &GridMeta, t: &Topology) -> Result<(), VdbError> {
    let class: &[u8] = if meta.extra.contains_key(SDF_KEY) { b"level set" } else { b"unknown" };
    let (min, max) = match Bounds::of(t)? {
        Bounds::Box { min, max } => (min, max),
        Bounds::Empty => EMPTY_BOX,
    };
    let vec3i = |c: Coord| c.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
    let entries: [(&str, &str, Vec<u8>); 7] = [
        ("class", "string", class.to_vec()),
        ("file_bbox_max", "vec3i", vec3i(max)),
        ("file_bbox_min", "vec3i", vec3i(min)),
        ("file_compression", "string", b"active values".to_vec()),
        ("file_mem_bytes", "int64", 0_i64.to_le_bytes().to_vec()),
        ("file_voxel_count", "int64", voxel_count(t)?.to_le_bytes().to_vec()),
        ("name", "string", meta.name.as_bytes().to_vec()),
    ];
    put_u32(out, 7);
    for (name, kind, value) in entries {
        put_str(out, name)?;
        put_str(out, kind)?;
        put_u32(out, u32_of(value.len(), "metadata value")?);
        out.extend(value);
    }
    Ok(())
}

/// The voxels of every leaf and tile.
fn voxel_count(t: &Topology) -> Result<i64, VdbError> {
    let too_large = || VdbError::TooLarge("voxel count");
    let leaves = i64::try_from(t.leaf_count()).map_err(|_| too_large())?;
    let mut count = leaves.checked_mul(512).ok_or_else(too_large)?;
    for n in t.tile_numbers() {
        let (level, _) = t.tile(n)?;
        count = count.checked_add(1_i64 << (3 * level.edge_log2())).ok_or_else(too_large)?;
    }
    Ok(count)
}

/// The transform (docs/formats.md §9.3): `AffineMap` and the 4×4 matrix by which OpenVDB
/// maps a row vector, its index v placed at brodo's voxel centre, v + ½.
fn put_transform(out: &mut Vec<u8>, a: &[[f64; 4]; 3]) -> Result<(), VdbError> {
    put_str(out, "AffineMap")?;
    let mut m = [[0.0_f64; 4]; 4];
    for (r, row) in a.iter().enumerate() {
        for (c, x) in row.iter().take(3).enumerate() {
            m[c][r] = *x;
        }
        m[3][r] = row[3] + 0.5 * (row[0] + row[1] + row[2]);
    }
    m[3][3] = 1.0;
    for x in m.iter().flatten() {
        out.extend(x.to_le_bytes());
    }
    Ok(())
}

fn put_coord(out: &mut Vec<u8>, c: Coord) {
    for x in c {
        out.extend(x.to_le_bytes());
    }
}

fn put_u32(out: &mut Vec<u8>, x: u32) {
    out.extend(x.to_le_bytes());
}

/// A string: its `u32` byte length and its bytes.
fn put_str(out: &mut Vec<u8>, s: &str) -> Result<(), VdbError> {
    put_u32(out, u32_of(s.len(), "string's length")?);
    out.extend(s.as_bytes());
    Ok(())
}

fn u32_of(n: usize, what: &'static str) -> Result<u32, VdbError> {
    u32::try_from(n).map_err(|_| VdbError::TooLarge(what))
}
