//! The cache profile (ARCHITECTURE.md §4.7, ADR-0016): grids with their metadata in one
//! file, each grid's frozen buffer on a 32-byte boundary. This module parses untrusted
//! bytes: it is listed in `UNTRUSTED_INPUT`, indexes nothing (CODING.md Rule 3), checks the
//! whole file before it answers, and is fuzzed (TESTING.md T6).
//!
//! Format version 1's bytes are in `docs/formats.md` §3, which this code follows.

use std::path::Path;

use zerocopy::IntoBytes;

use crate::error::{CacheError, FormatError};
use crate::field::{Field, Stored};
use crate::frozen::{BLOCK, Block, FrozenGrid, aligned, freeze};
use crate::meta::{Bounds, GridMeta};
use crate::topology::Topology;
use crate::view::GridView;

/// "BRDC", read as a little-endian `u32`.
const MAGIC: u32 = u32::from_le_bytes(*b"BRDC");
/// The format version this build writes and reads (ADR-0016).
const VERSION: u32 = 1;

/// A grid ready to be written: its metadata, bounds, value type and frozen buffer.
#[derive(Debug, Clone)]
pub struct CacheEntry {
    meta: GridMeta,
    bounds: Bounds,
    value_type: u32,
    frozen: FrozenGrid,
}

impl CacheEntry {
    /// Freezes a topology and a field on it, with its metadata, for a cache file.
    pub fn new<T: Stored>(meta: &GridMeta, t: &Topology, f: &Field<T>) -> Result<Self, CacheError> {
        meta.check()?;
        Ok(Self {
            meta: meta.clone(),
            bounds: Bounds::of(t)?,
            value_type: T::TYPE_CODE,
            frozen: freeze(t, f)?,
        })
    }
}

fn u32_of(n: usize) -> Result<u32, CacheError> {
    u32::try_from(n).map_err(|_| CacheError::TooLarge)
}

fn put_u32(out: &mut Vec<u8>, x: u32) {
    out.extend_from_slice(&x.to_le_bytes());
}

fn put_str(out: &mut Vec<u8>, s: &str) -> Result<(), CacheError> {
    put_u32(out, u32_of(s.len())?);
    out.extend_from_slice(s.as_bytes());
    out.resize(out.len().next_multiple_of(4), 0);
    Ok(())
}

/// A grid's metadata and bounds, as stored.
fn encode_meta(m: &GridMeta, b: Bounds) -> Result<Vec<u8>, CacheError> {
    let mut out = Vec::new();
    put_str(&mut out, &m.name)?;
    put_str(&mut out, &m.unit)?;
    for x in m.voxel_size.iter().chain(m.transform.iter().flatten()) {
        out.extend_from_slice(&x.to_le_bytes());
    }
    let (kind, corners) = match b {
        Bounds::Empty => (0, [[0; 3]; 2]),
        Bounds::Box { min, max } => (1, [min, max]),
    };
    put_u32(&mut out, kind);
    for c in corners.iter().flatten() {
        out.extend_from_slice(&c.to_le_bytes());
    }
    put_u32(&mut out, u32_of(m.extra.len())?);
    for (k, v) in &m.extra {
        put_str(&mut out, k)?;
        put_str(&mut out, v)?;
    }
    Ok(out)
}

/// The bytes of a cache file holding `entries`, in order.
pub fn to_bytes(entries: &[CacheEntry]) -> Result<Vec<u8>, CacheError> {
    let metas =
        entries.iter().map(|e| encode_meta(&e.meta, e.bounds)).collect::<Result<Vec<_>, _>>()?;
    let mut directory = Vec::new();
    let mut at = 1 + entries.len();
    let mut places = Vec::new();
    for (e, m) in entries.iter().zip(&metas) {
        let grid_blocks = e.frozen.as_bytes().len() / BLOCK;
        let (meta_at, grid_at) = (at, at + m.len().div_ceil(BLOCK));
        at = grid_at + grid_blocks;
        for x in [
            u32_of(meta_at)?,
            u32_of(m.len())?,
            u32_of(grid_at)?,
            u32_of(grid_blocks)?,
            e.value_type,
            0,
            0,
            0,
        ] {
            put_u32(&mut directory, x);
        }
        places.push((meta_at, grid_at));
    }
    let mut out = Vec::new();
    for x in [MAGIC, VERSION, u32_of(entries.len())?, 0, 0, 0, 0, 0] {
        put_u32(&mut out, x);
    }
    out.extend_from_slice(&directory);
    for ((e, m), (meta_at, grid_at)) in entries.iter().zip(&metas).zip(places) {
        out.resize(meta_at * BLOCK, 0);
        out.extend_from_slice(m);
        out.resize(grid_at * BLOCK, 0);
        out.extend_from_slice(e.frozen.as_bytes());
    }
    Ok(out)
}

/// Writes a cache file holding `entries`.
pub fn write(path: &Path, entries: &[CacheEntry]) -> Result<(), CacheError> {
    std::fs::write(path, to_bytes(entries)?)?;
    Ok(())
}

/// Reads bytes in order, refusing to read past their end.
struct Cursor<'a> {
    rest: &'a [u8],
    what: &'static str,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], CacheError> {
        let (a, b) = self.rest.split_at_checked(n).ok_or(CacheError::Truncated(self.what))?;
        self.rest = b;
        Ok(a)
    }

    fn word(&mut self) -> Result<[u8; 4], CacheError> {
        <[u8; 4]>::try_from(self.take(4)?).map_err(|_| CacheError::Truncated(self.what))
    }

    fn u32(&mut self) -> Result<u32, CacheError> {
        Ok(u32::from_le_bytes(self.word()?))
    }

    fn i32(&mut self) -> Result<i32, CacheError> {
        Ok(i32::from_le_bytes(self.word()?))
    }

    fn f64(&mut self) -> Result<f64, CacheError> {
        let b = <[u8; 8]>::try_from(self.take(8)?).map_err(|_| CacheError::Truncated(self.what))?;
        Ok(f64::from_le_bytes(b))
    }

    fn string(&mut self) -> Result<String, CacheError> {
        let len = usize::try_from(self.u32()?).map_err(|_| CacheError::Truncated(self.what))?;
        let bytes = self.take(len)?;
        self.take(len.next_multiple_of(4) - len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| CacheError::BadText(self.what))
    }
}

fn decode_meta(bytes: &[u8]) -> Result<(GridMeta, Bounds), CacheError> {
    let mut c = Cursor { rest: bytes, what: "metadata" };
    let (name, unit) = (c.string()?, c.string()?);
    let mut floats = [0.0; 15];
    for x in &mut floats {
        *x = c.f64()?;
    }
    let [v0, v1, v2, t @ ..] = floats;
    let [a0, a1, a2, a3, b0, b1, b2, b3, d0, d1, d2, d3] = t;
    let kind = c.u32()?;
    let mut corners = [0_i32; 6];
    for x in &mut corners {
        *x = c.i32()?;
    }
    let [x0, y0, z0, x1, y1, z1] = corners;
    let bounds = match kind {
        0 => Bounds::Empty,
        1 => Bounds::Box { min: [x0, y0, z0], max: [x1, y1, z1] },
        _ => return Err(CacheError::Inconsistent("bounds")),
    };
    let mut extra = std::collections::BTreeMap::new();
    for _ in 0..c.u32()? {
        let (k, v) = (c.string()?, c.string()?);
        if extra.insert(k, v).is_some() {
            return Err(CacheError::Inconsistent("metadata's keys"));
        }
    }
    if !c.rest.is_empty() {
        return Err(CacheError::Inconsistent("metadata"));
    }
    let meta = GridMeta {
        name,
        voxel_size: [v0, v1, v2],
        unit,
        transform: [[a0, a1, a2, a3], [b0, b1, b2, b3], [d0, d1, d2, d3]],
        extra,
    };
    // Only the bytes brodo writes for what was read: string padding and an empty box's
    // corners zero, keys in order.
    if encode_meta(&meta, bounds)? != bytes {
        return Err(CacheError::Inconsistent("metadata"));
    }
    meta.check()?;
    Ok((meta, bounds))
}

/// One grid of a read file.
#[derive(Debug, Clone)]
pub(crate) struct Entry {
    meta: GridMeta,
    bounds: Bounds,
    value_type: u32,
    /// The frozen buffer's byte range in the file.
    grid: std::ops::Range<usize>,
}

/// A cache file read into a 32-byte aligned buffer and checked whole.
#[derive(Debug)]
pub struct CacheFile {
    pub(crate) storage: Storage,
    pub(crate) grids: Vec<Entry>,
}

/// Where a read file's bytes are: copied into an aligned buffer, or mapped.
#[derive(Debug)]
pub(crate) enum Storage {
    Copied(Vec<Block>),
    #[cfg(feature = "mmap")]
    Mapped(memmap2::Mmap),
}

impl Storage {
    fn bytes(&self) -> &[u8] {
        match self {
            Storage::Copied(blocks) => blocks.as_bytes(),
            #[cfg(feature = "mmap")]
            Storage::Mapped(map) => map,
        }
    }
}

/// Checks a grid's frozen buffer with the type its directory names.
fn check_grid(bytes: &[u8], value_type: u32) -> Result<(), FormatError> {
    fn view<T: Stored>(b: &[u8]) -> Result<(), FormatError> {
        GridView::<T>::new(b).map(|_| ())
    }
    match value_type {
        t if t == f32::TYPE_CODE => view::<f32>(bytes),
        t if t == f64::TYPE_CODE => view::<f64>(bytes),
        t if t == u8::TYPE_CODE => view::<u8>(bytes),
        t if t == u32::TYPE_CODE => view::<u32>(bytes),
        t if t == <[f32; 3]>::TYPE_CODE => view::<[f32; 3]>(bytes),
        t if t == <[f64; 3]>::TYPE_CODE => view::<[f64; 3]>(bytes),
        found => Err(FormatError::WrongValueType { expected: 0, found }),
    }
}

/// A range of `len` bytes from block `at`, which must be byte `expected`, where the writer
/// puts it, and end within `total`.
fn range(
    at: u32,
    len: usize,
    expected: usize,
    total: usize,
    what: &'static str,
) -> Result<std::ops::Range<usize>, CacheError> {
    let start = usize::try_from(at)
        .ok()
        .and_then(|a| a.checked_mul(BLOCK))
        .ok_or(CacheError::Inconsistent(what))?;
    let end = start.checked_add(len).ok_or(CacheError::Inconsistent(what))?;
    if start != expected {
        return Err(CacheError::Inconsistent(what));
    }
    if end > total {
        return Err(CacheError::Truncated(what));
    }
    Ok(start..end)
}

/// Reads the header block: the magic number, this build's format version, and unused
/// words of zero. Returns the grid count.
fn header(c: &mut Cursor<'_>) -> Result<u32, CacheError> {
    let magic = c.u32()?;
    if magic != MAGIC {
        return Err(CacheError::BadMagic(magic));
    }
    let version = c.u32()?;
    if version != VERSION {
        return Err(CacheError::UnsupportedVersion(version));
    }
    let count = c.u32()?;
    if !zero(c.take(BLOCK - 12)?) {
        return Err(CacheError::Inconsistent("header"));
    }
    Ok(count)
}

/// Checks a cache file's bytes whole: header, directory, every grid's metadata and frozen
/// buffer, each where the writer puts it, with no gap, padding of zeros, and nothing after
/// the last grid (ADR-0016). The bytes start on a 32-byte boundary.
pub(crate) fn index(all: &[u8]) -> Result<Vec<Entry>, CacheError> {
    if all.as_ptr().align_offset(BLOCK) != 0 {
        return Err(CacheError::Inconsistent("alignment"));
    }
    let mut c = Cursor { rest: all, what: "header" };
    let count = header(&mut c)?;
    c.what = "directory";
    let mut after = usize::try_from(count)
        .ok()
        .and_then(|n| n.checked_add(1))
        .and_then(|n| n.checked_mul(BLOCK))
        .ok_or(CacheError::Truncated("directory"))?;
    let mut grids = Vec::new();
    for index in 0..count {
        let [meta_at, meta_len, grid_at, grid_blocks, value_type, r0, r1, r2] =
            [(); 8].map(|()| c.u32());
        if [r0?, r1?, r2?] != [0; 3] {
            return Err(CacheError::Inconsistent("directory"));
        }
        let meta_len =
            usize::try_from(meta_len?).map_err(|_| CacheError::Inconsistent("directory"))?;
        let meta = range(meta_at?, meta_len, after, all.len(), "metadata")?;
        let grid_len = usize::try_from(grid_blocks?)
            .ok()
            .and_then(|b| b.checked_mul(BLOCK))
            .ok_or(CacheError::Inconsistent("directory"))?;
        let grid = range(grid_at?, grid_len, meta.end.next_multiple_of(BLOCK), all.len(), "grid")?;
        if !zero(all.get(meta.end..grid.start).ok_or(CacheError::Truncated("metadata"))?) {
            return Err(CacheError::Inconsistent("padding"));
        }
        let (meta, bounds) =
            decode_meta(all.get(meta.clone()).ok_or(CacheError::Truncated("metadata"))?)?;
        let value_type = value_type?;
        let index = usize::try_from(index).map_err(|_| CacheError::TooLarge)?;
        check_grid(all.get(grid.clone()).ok_or(CacheError::Truncated("grid"))?, value_type)
            .map_err(|error| CacheError::Grid { index, error })?;
        after = grid.end;
        grids.push(Entry { meta, bounds, value_type, grid });
    }
    if after != all.len() {
        return Err(CacheError::Inconsistent("end"));
    }
    Ok(grids)
}

fn zero(bytes: &[u8]) -> bool {
    bytes.iter().all(|b| *b == 0)
}

impl CacheFile {
    /// Reads and checks a cache file of at most `limit` bytes; a longer one is refused by
    /// its length before any of it is read (ADR-0036).
    pub fn read(path: &Path, limit: u64) -> Result<Self, CacheError> {
        let length = std::fs::metadata(path)?.len();
        if length > limit {
            return Err(CacheError::FileTooLarge { length, limit });
        }
        Self::from_bytes(&std::fs::read(path)?)
    }

    /// Checks bytes whole as a cache file, copying them to an aligned buffer.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CacheError> {
        if !bytes.len().is_multiple_of(BLOCK) {
            return Err(CacheError::Inconsistent("length"));
        }
        let blocks = aligned(bytes);
        let grids = index(blocks.as_bytes())?;
        Ok(Self { storage: Storage::Copied(blocks), grids })
    }

    fn entry(&self, i: usize) -> Result<&Entry, CacheError> {
        self.grids.get(i).ok_or(CacheError::NoGrid(i))
    }

    /// The number of grids.
    pub fn grid_count(&self) -> usize {
        self.grids.len()
    }

    /// A grid's metadata.
    pub fn meta(&self, i: usize) -> Result<&GridMeta, CacheError> {
        Ok(&self.entry(i)?.meta)
    }

    /// The voxels a grid's leaves and tiles cover.
    pub fn bounds(&self, i: usize) -> Result<Bounds, CacheError> {
        Ok(self.entry(i)?.bounds)
    }

    /// The type code of a grid's values (`Stored::TYPE_CODE`).
    pub fn value_type(&self, i: usize) -> Result<u32, CacheError> {
        Ok(self.entry(i)?.value_type)
    }

    /// A view of a grid's values of type `T`.
    pub fn view<T: Stored>(&self, i: usize) -> Result<GridView<'_, T>, CacheError> {
        let e = self.entry(i)?;
        let bytes =
            self.storage.bytes().get(e.grid.clone()).ok_or(CacheError::Truncated("grid"))?;
        GridView::new(bytes).map_err(|error| CacheError::Grid { index: i, error })
    }
}
