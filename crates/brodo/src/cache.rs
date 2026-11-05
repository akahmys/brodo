//! The cache profile (ARCHITECTURE.md §4.7, ADR-0016): grids with their metadata in one
//! file, each grid's frozen buffer on a 32-byte boundary. This module parses untrusted
//! bytes: it is listed in `UNTRUSTED_INPUT`, indexes nothing (CODING.md Rule 3), checks the
//! whole file before it answers, and is fuzzed (TESTING.md T6).
//!
//! The bytes are in `docs/formats.md` §3, which this code follows: version 2, which holds
//! entries of bytes beside grids and has its bounds checked (ADR-0051, ADR-0052). Version 1
//! is neither read nor written (ADR-0054).

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use zerocopy::{FromZeros, IntoBytes};

use crate::error::{CacheError, FormatError};
use crate::field::{AnyField, Field, ForType, LEAF_VOXELS, Stored, ValueType, with_type};
use crate::frozen::{BLOCK, Block, FrozenGrid, aligned, freeze};
use crate::meta::{Bounds, GridMeta, SDF_KEY};
use crate::topology::Topology;
use crate::view::GridView;

use crate::kind::CACHE_MAGIC as MAGIC;
/// The format version this build reads and writes.
pub(crate) const VERSION: u32 = 2;
/// A directory entry's kinds (docs/formats.md §3.2).
pub(crate) const KIND_GRID: u32 = 0;
pub(crate) const KIND_BYTES: u32 = 1;

/// An entry ready to be written: a grid, with its metadata, bounds, value type and frozen
/// buffer; or, at version 2, an entry of bytes, with its name and free pairs.
#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub(crate) payload: Payload,
}

/// What an entry holds.
#[derive(Debug, Clone)]
pub(crate) enum Payload {
    Grid { meta: GridMeta, bounds: Bounds, value_type: u32, frozen: FrozenGrid },
    Bytes { name: String, extra: BTreeMap<String, String>, bytes: Vec<u8> },
}

/// The prefix of the keys brodo's formats on the cache profile keep: Hermite data's, the
/// SDF convention's, and those of formats to come (docs/formats.md §3.3, ADR-0053).
pub const RESERVED_PREFIX: &str = "brodo.";

/// Whether `key` is one brodo's formats keep.
pub(crate) fn reserved(key: &str) -> bool {
    key.starts_with(RESERVED_PREFIX)
}

impl CacheEntry {
    /// Freezes a topology and a field on it, with its metadata, for a cache file. The
    /// metadata may hold no key beginning `brodo.` (`RESERVED_PREFIX`) but the SDF
    /// convention's, `SDF_KEY`: the others are brodo's formats', a Hermite set's written by
    /// `Hermite::entries`, so that a grid is not taken for one by mistake.
    pub fn new<T: Stored>(meta: &GridMeta, t: &Topology, f: &Field<T>) -> Result<Self, CacheError> {
        if let Some(key) = meta.extra.keys().find(|k| reserved(k) && *k != SDF_KEY) {
            return Err(CacheError::ReservedKey(key.clone()));
        }
        Self::with_reserved_keys(meta, t, f)
    }

    /// As `new`, the metadata free to hold keys beginning `brodo.`: for a Hermite set's own
    /// grids, or a file made on purpose to break its format, as a test of a reader. Hidden,
    /// it promises nothing to a user (ADR-0057).
    #[doc(hidden)]
    pub fn with_reserved_keys<T: Stored>(
        meta: &GridMeta,
        t: &Topology,
        f: &Field<T>,
    ) -> Result<Self, CacheError> {
        meta.check()?;
        Ok(Self {
            payload: Payload::Grid {
                meta: meta.clone(),
                bounds: Bounds::of(t)?,
                value_type: T::VALUE_TYPE.code(),
                frozen: freeze(t, f)?,
            },
        })
    }

    /// An entry of bytes, named, with free pairs, which a file of version 2 holds beside its
    /// grids for a format built on it (docs/formats.md §3); the pairs may hold no key
    /// beginning `brodo.`, as `new`'s may not, the SDF convention's included.
    pub fn bytes(
        name: &str,
        extra: BTreeMap<String, String>,
        bytes: Vec<u8>,
    ) -> Result<Self, CacheError> {
        if let Some(key) = extra.keys().find(|k| reserved(k)) {
            return Err(CacheError::ReservedKey(key.clone()));
        }
        Ok(Self::bytes_with_reserved_keys(name, extra, bytes))
    }

    /// As `bytes`, the pairs free to hold keys beginning `brodo.`: for a Hermite set's own
    /// entry, or a file made on purpose to break its format, as a test of a reader. Hidden,
    /// it promises nothing to a user (ADR-0057).
    #[doc(hidden)]
    pub fn bytes_with_reserved_keys(
        name: &str,
        extra: BTreeMap<String, String>,
        bytes: Vec<u8>,
    ) -> Self {
        Self { payload: Payload::Bytes { name: name.to_owned(), extra, bytes } }
    }

    /// A grid a file read holds (`CacheFile::grids`), to be written again as it was: its
    /// metadata, keys beginning `brodo.` among them, its bounds, its value type and its
    /// frozen buffer, copied, not thawed nor frozen again. A Hermite set's sign grid or an
    /// SDF read from a cache file or an archive is so written to another whole.
    pub fn copy(grid: GridEntry<'_>) -> Self {
        Self {
            payload: Payload::Grid {
                meta: grid.meta.clone(),
                bounds: grid.bounds,
                value_type: grid.value_type.code(),
                frozen: FrozenGrid::of_blocks(aligned(grid.bytes)),
            },
        }
    }

    /// An entry of bytes a file read holds (`CacheFile::bytes_entries`), to be written again
    /// as it was: its name, its free pairs, keys beginning `brodo.` among them, and its
    /// bytes, copied. A Hermite set's crossings are so written to another file whole.
    pub fn copy_bytes(entry: BytesEntry<'_>) -> Self {
        Self::bytes_with_reserved_keys(entry.name, entry.extra.clone(), entry.bytes.to_vec())
    }

    /// The entry as a file places it: its metadata as stored, and its part.
    pub(crate) fn part(&self) -> Result<(Vec<u8>, Part), CacheError> {
        let (meta, value_type, kind, len) = match &self.payload {
            Payload::Grid { meta, bounds, value_type, frozen } => {
                (encode_meta(meta, *bounds)?, *value_type, KIND_GRID, frozen.as_bytes().len())
            }
            Payload::Bytes { name, extra, bytes } => {
                (encode_bytes_meta(name, bytes.len(), extra)?, 0, KIND_BYTES, bytes.len())
            }
        };
        let part =
            Part { meta_len: meta.len(), grid_blocks: len.div_ceil(BLOCK), value_type, kind };
        Ok((meta, part))
    }

    /// Writes the entry's payload to `out` as a file holds it: a grid's frozen buffer, or
    /// bytes, zero-padded to a block. The bytes written.
    pub(crate) fn write_payload(&self, out: &mut impl std::io::Write) -> std::io::Result<usize> {
        match &self.payload {
            Payload::Grid { frozen, .. } => {
                out.write_all(frozen.as_bytes())?;
                Ok(frozen.as_bytes().len())
            }
            Payload::Bytes { bytes, .. } => {
                let padded = bytes.len().next_multiple_of(BLOCK);
                out.write_all(bytes)?;
                out.write_all(&vec![0; padded - bytes.len()])?;
                Ok(padded)
            }
        }
    }
}

/// An entry of bytes' metadata as stored: its name, its byte length, its free pairs.
pub(crate) fn encode_bytes_meta(
    name: &str,
    len: usize,
    extra: &BTreeMap<String, String>,
) -> Result<Vec<u8>, CacheError> {
    let mut out = Vec::new();
    put_str(&mut out, name)?;
    out.extend_from_slice(&u64::try_from(len).map_err(|_| CacheError::TooLarge)?.to_le_bytes());
    put_pairs(&mut out, extra)?;
    Ok(out)
}

fn put_pairs(out: &mut Vec<u8>, extra: &BTreeMap<String, String>) -> Result<(), CacheError> {
    put_u32(out, u32_of(extra.len())?);
    for (k, v) in extra {
        put_str(out, k)?;
        put_str(out, v)?;
    }
    Ok(())
}

pub(crate) fn u32_of(n: usize) -> Result<u32, CacheError> {
    u32::try_from(n).map_err(|_| CacheError::TooLarge)
}

pub(crate) fn put_u32(out: &mut Vec<u8>, x: u32) {
    out.extend_from_slice(&x.to_le_bytes());
}

fn put_str(out: &mut Vec<u8>, s: &str) -> Result<(), CacheError> {
    put_u32(out, u32_of(s.len())?);
    out.extend_from_slice(s.as_bytes());
    out.resize(out.len().next_multiple_of(4), 0);
    Ok(())
}

/// A grid's metadata and bounds, as stored.
pub(crate) fn encode_meta(m: &GridMeta, b: Bounds) -> Result<Vec<u8>, CacheError> {
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
    put_pairs(&mut out, &m.extra)?;
    Ok(out)
}

/// The bytes of a cache file holding `entries`, in order.
pub fn to_bytes(entries: &[CacheEntry]) -> Result<Vec<u8>, CacheError> {
    let mut out = Vec::new();
    emit(entries, &mut out)?;
    Ok(out)
}

/// Writes zeros to `out` from `*written` bytes to `to`.
pub(crate) fn pad_to(
    out: &mut impl std::io::Write,
    to: usize,
    written: &mut usize,
) -> std::io::Result<()> {
    let zeros = to.saturating_sub(*written);
    *written = to.max(*written);
    out.write_all(&vec![0; zeros])
}

/// An entry as the cache file places it: its metadata's length in bytes, its payload's in
/// blocks, its value type and its kind.
pub(crate) struct Part {
    pub(crate) meta_len: usize,
    pub(crate) grid_blocks: usize,
    pub(crate) value_type: u32,
    pub(crate) kind: u32,
}

/// The header and directory of a cache file of `parts`, and the block each payload starts
/// at; each metadata starts at the block after the part before it.
pub(crate) fn head(parts: &[Part]) -> Result<(Vec<u8>, Vec<usize>), CacheError> {
    let mut head = Vec::new();
    for x in [MAGIC, VERSION, u32_of(parts.len())?, 0, 0, 0, 0, 0] {
        put_u32(&mut head, x);
    }
    let mut at = 1 + parts.len();
    let mut places = Vec::new();
    for p in parts {
        let (meta_at, grid_at) = (at, at + p.meta_len.div_ceil(BLOCK));
        at = grid_at + p.grid_blocks;
        for x in [
            u32_of(meta_at)?,
            u32_of(p.meta_len)?,
            u32_of(grid_at)?,
            u32_of(p.grid_blocks)?,
            p.value_type,
            p.kind,
            0,
            0,
        ] {
            put_u32(&mut head, x);
        }
        places.push(grid_at);
    }
    u32_of(at)?;
    Ok((head, places))
}

/// Writes the file holding `entries` to `out`, part by part.
fn emit(entries: &[CacheEntry], out: &mut impl std::io::Write) -> Result<(), CacheError> {
    let (metas, parts): (Vec<_>, Vec<_>) =
        entries.iter().map(CacheEntry::part).collect::<Result<Vec<_>, _>>()?.into_iter().unzip();
    let (head, places) = head(&parts)?;
    out.write_all(&head)?;
    let mut written = head.len();
    // Each metadata starts where the part before it ends, the head or a payload, at a block.
    for ((e, m), at) in entries.iter().zip(&metas).zip(places) {
        out.write_all(m)?;
        written += m.len();
        pad_to(out, at * BLOCK, &mut written)?;
        written += e.write_payload(out)?;
    }
    Ok(())
}

/// The cache files this process has begun to write, numbering their hidden files.
static WRITES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Writes a cache file holding `entries`. The bytes go to a hidden file beside `path`,
/// `.<name>.<process id>.<n>.tmp`, n counting the process's writes so that two at once do
/// not share it, made new, flushed to the disk and then renamed over `path`: a reader,
/// a process mapping the old file among them, sees the old file or the new one whole, and a
/// write that fails leaves the old one. The directory must be writable; a process killed
/// while writing may leave the hidden file behind.
pub fn write(path: &Path, entries: &[CacheEntry]) -> Result<(), CacheError> {
    replace(path, |out| emit(entries, out))
}

/// Writes the bytes `emit` gives to a hidden file beside `path` and renames it over
/// `path`, as `write` states.
pub(crate) fn replace<E: From<std::io::Error>>(
    path: &Path,
    emit: impl FnOnce(&mut std::io::BufWriter<std::fs::File>) -> Result<(), E>,
) -> Result<(), E> {
    use std::io::Write;
    let name = path.file_name().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "the path names no file")
    })?;
    let mut hidden = std::ffi::OsString::from(".");
    hidden.push(name);
    let n = WRITES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    hidden.push(format!(".{}.{n}.tmp", std::process::id()));
    let temp = path.with_file_name(hidden);
    let written = (|| -> Result<(), E> {
        let made = std::fs::OpenOptions::new().write(true).create_new(true).open(&temp)?;
        let mut file = std::io::BufWriter::new(made);
        emit(&mut file)?;
        file.flush()?;
        file.get_ref().sync_all()?;
        drop(file);
        std::fs::rename(&temp, path)?;
        Ok(())
    })();
    if written.is_err() {
        // The hidden file may hold part of the bytes; the old file is untouched.
        let _ignored = std::fs::remove_file(&temp);
    }
    written
}

/// Reads bytes in order, refusing to read past their end.
pub(crate) struct Cursor<'a> {
    pub(crate) rest: &'a [u8],
    pub(crate) what: &'static str,
}

impl<'a> Cursor<'a> {
    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8], CacheError> {
        let (a, b) = self.rest.split_at_checked(n).ok_or(CacheError::Truncated(self.what))?;
        self.rest = b;
        Ok(a)
    }

    fn word(&mut self) -> Result<[u8; 4], CacheError> {
        <[u8; 4]>::try_from(self.take(4)?).map_err(|_| CacheError::Truncated(self.what))
    }

    pub(crate) fn u32(&mut self) -> Result<u32, CacheError> {
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

pub(crate) fn decode_meta(bytes: &[u8]) -> Result<(GridMeta, Bounds), CacheError> {
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
        1 if x0 <= x1 && y0 <= y1 && z0 <= z1 => {
            Bounds::Box { min: [x0, y0, z0], max: [x1, y1, z1] }
        }
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

/// One entry of bytes of a read file.
#[derive(Debug, Clone)]
pub(crate) struct BytesIndex {
    name: String,
    extra: BTreeMap<String, String>,
    /// The bytes' range in the file, padding left out.
    range: std::ops::Range<usize>,
}

/// An entry of bytes a cache file holds beside its grids (docs/formats.md §3).
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct BytesEntry<'a> {
    /// Its name.
    pub name: &'a str,
    /// Its free pairs.
    pub extra: &'a BTreeMap<String, String>,
    /// Its bytes.
    pub bytes: &'a [u8],
}

/// What thawing a tile is counted to make, and a leaf beside its values (ADR-0050, measured
/// in `spikes/thawmemory/`, 2026-11-03).
const THAW_TILE: u64 = 256;
const THAW_LEAF: u64 = 1024;

/// One grid of a read file.
#[derive(Debug, Clone)]
pub(crate) struct Entry {
    meta: GridMeta,
    bounds: Bounds,
    value_type: ValueType,
    /// The frozen buffer's byte range in the file.
    pub(crate) grid: std::ops::Range<usize>,
}

/// A cache file read into a 32-byte aligned buffer and checked whole.
#[derive(Debug)]
pub struct CacheFile {
    pub(crate) storage: Storage,
    pub(crate) index: Index,
}

/// What a file holds, found by checking it whole.
#[derive(Debug, Clone)]
pub(crate) struct Index {
    pub(crate) version: u32,
    pub(crate) grids: Vec<Entry>,
    pub(crate) bytes: Vec<BytesIndex>,
}

/// Where a read file's bytes are: copied into an aligned buffer, or mapped.
#[derive(Debug)]
pub(crate) enum Storage {
    Copied(Vec<Block>),
    #[cfg(feature = "mmap")]
    Mapped(memmap2::Mmap),
}

impl Storage {
    pub(crate) fn bytes(&self) -> &[u8] {
        match self {
            Storage::Copied(blocks) => blocks.as_bytes(),
            #[cfg(feature = "mmap")]
            Storage::Mapped(map) => map,
        }
    }
}

/// Checks a grid's frozen buffer with the type its directory names: the bounds its leaves
/// and tiles cover.
fn check_grid(bytes: &[u8], value_type: ValueType) -> Result<Bounds, FormatError> {
    struct Check<'a>(&'a [u8]);
    impl ForType for Check<'_> {
        type Out = Result<Bounds, FormatError>;
        fn run<T: Stored>(self) -> Self::Out {
            GridView::<T>::new(self.0)?.bounds()
        }
    }
    with_type(value_type, Check(bytes))
}

/// A range of `len` bytes from block `at`, which must be byte `expected`, where the writer
/// puts it, and end within `total`.
pub(crate) fn range(
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

/// Reads the header block: the magic number, a format version this build reads, and unused
/// words of zero. Returns the entry count.
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

/// An entry of bytes' metadata: its name, its byte length and its free pairs, in the bytes
/// brodo writes for them.
fn decode_bytes_meta(bytes: &[u8]) -> Result<(String, u64, BTreeMap<String, String>), CacheError> {
    let mut c = Cursor { rest: bytes, what: "metadata" };
    let name = c.string()?;
    let len = <[u8; 8]>::try_from(c.take(8)?).map_err(|_| CacheError::Truncated("metadata"))?;
    let len = u64::from_le_bytes(len);
    let mut extra = BTreeMap::new();
    for _ in 0..c.u32()? {
        let (k, v) = (c.string()?, c.string()?);
        if extra.insert(k, v).is_some() {
            return Err(CacheError::Inconsistent("metadata's keys"));
        }
    }
    let len_usize = usize::try_from(len).map_err(|_| CacheError::TooLarge)?;
    if !c.rest.is_empty() || encode_bytes_meta(&name, len_usize, &extra)? != bytes {
        return Err(CacheError::Inconsistent("metadata"));
    }
    Ok((name, len, extra))
}

/// Checks a cache file's bytes whole: header, directory, every entry's metadata and
/// payload, each where the writer puts it, with no gap, padding of zeros, and nothing after
/// the last (ADR-0016, docs/formats.md §3.4). The bytes start on a 32-byte
/// boundary.
pub(crate) fn index(all: &[u8]) -> Result<Index, CacheError> {
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
    let mut index = Index { version: VERSION, grids: Vec::new(), bytes: Vec::new() };
    for place in 0..count {
        let [meta_at, meta_len, at, blocks, value_type, kind, r1, r2] = [(); 8].map(|()| c.u32());
        let kind = kind?;
        let kind_ok = kind == KIND_GRID || kind == KIND_BYTES;
        if [r1?, r2?] != [0; 2] || !kind_ok {
            return Err(CacheError::Inconsistent("directory"));
        }
        let meta_len =
            usize::try_from(meta_len?).map_err(|_| CacheError::Inconsistent("directory"))?;
        let meta = range(meta_at?, meta_len, after, all.len(), "metadata")?;
        let len = usize::try_from(blocks?)
            .ok()
            .and_then(|b| b.checked_mul(BLOCK))
            .ok_or(CacheError::Inconsistent("directory"))?;
        let payload = range(at?, len, meta.end.next_multiple_of(BLOCK), all.len(), "grid")?;
        if !zero(all.get(meta.end..payload.start).ok_or(CacheError::Truncated("metadata"))?) {
            return Err(CacheError::Inconsistent("padding"));
        }
        let meta = all.get(meta).ok_or(CacheError::Truncated("metadata"))?;
        after = payload.end;
        if kind == KIND_BYTES {
            index.bytes.push(bytes_entry(all, meta, payload, value_type?)?);
            continue;
        }
        let place = usize::try_from(place).map_err(|_| CacheError::TooLarge)?;
        index.grids.push(grid_entry(all, meta, payload, value_type?, place)?);
    }
    if after != all.len() {
        return Err(CacheError::Inconsistent("end"));
    }
    Ok(index)
}

/// The grid at `payload`, the entry at `place` in the directory, its metadata `meta`,
/// checked as docs/formats.md §3.4 states, its bounds those its leaves and tiles cover.
fn grid_entry(
    all: &[u8],
    meta: &[u8],
    payload: std::ops::Range<usize>,
    value_type: u32,
    place: usize,
) -> Result<Entry, CacheError> {
    let (meta, bounds) = decode_meta(meta)?;
    let grid = all.get(payload.clone()).ok_or(CacheError::Truncated("grid"))?;
    let refused = |error| CacheError::Grid { index: place, error };
    let value_type = ValueType::from_code(value_type)
        .ok_or(refused(FormatError::UnknownValueType(value_type)))?;
    let found = check_grid(grid, value_type).map_err(refused)?;
    if found != bounds {
        return Err(CacheError::Inconsistent("bounds"));
    }
    Ok(Entry { meta, bounds, value_type, grid: payload })
}

/// An entry of bytes at `payload`, checked as docs/formats.md §3.4, rule 7, states.
fn bytes_entry(
    all: &[u8],
    meta: &[u8],
    payload: std::ops::Range<usize>,
    value_type: u32,
) -> Result<BytesIndex, CacheError> {
    let (name, len, extra) = decode_bytes_meta(meta)?;
    let len = usize::try_from(len).map_err(|_| CacheError::TooLarge)?;
    let blocks = payload.len();
    let within = len <= blocks && blocks - len < BLOCK;
    if value_type != 0 || !within {
        return Err(CacheError::Inconsistent("entry of bytes"));
    }
    let end = payload.start + len;
    if !zero(all.get(end..payload.end).ok_or(CacheError::Truncated("entry of bytes"))?) {
        return Err(CacheError::Inconsistent("padding"));
    }
    Ok(BytesIndex { name, extra, range: payload.start..end })
}

pub(crate) fn zero(bytes: &[u8]) -> bool {
    bytes.iter().all(|b| *b == 0)
}

/// A file's bytes, of at most `limit`: a longer file is refused by its length before any of
/// it is read, or by the bytes read where its length says less than it holds, as
/// `too_large(length, limit)` (ADR-0036).
#[cfg(any(feature = "archive", feature = "vdb"))]
pub(crate) fn read_within<E: From<std::io::Error>>(
    path: &Path,
    limit: u64,
    too_large: impl Fn(u64, u64) -> E,
) -> Result<Vec<u8>, E> {
    let file = std::fs::File::open(path)?;
    let length = file.metadata()?.len();
    if length > limit {
        return Err(too_large(length, limit));
    }
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1)).read_to_end(&mut bytes)?;
    let read = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if read > limit {
        return Err(too_large(read, limit));
    }
    Ok(bytes)
}

/// The length rule every reader of a cache file holds before it reads one: whole blocks,
/// and at least the header's (docs/formats.md §3.4 rules 1 and 2). `CacheFile::map` holds
/// it before mapping, so that a file of no bytes is refused as the others refuse it rather
/// than by the mapping.
pub(crate) fn whole_blocks(length: u64) -> Result<(), CacheError> {
    if !length.is_multiple_of(BLOCK as u64) {
        return Err(CacheError::Inconsistent("length"));
    }
    if length == 0 {
        return Err(CacheError::Truncated("header"));
    }
    Ok(())
}

/// Reads `source` to its end into `blocks`, from their start, growing them as it needs, a
/// block at least: the bytes read. A read interrupted is tried again.
fn fill(source: &mut impl Read, blocks: &mut Vec<Block>) -> std::io::Result<usize> {
    let mut filled = 0_usize;
    loop {
        if filled == blocks.len() * BLOCK {
            blocks.resize((blocks.len() * 2).max(1), Block::new_zeroed());
        }
        let Some(free) = blocks.as_mut_bytes().get_mut(filled..) else { return Ok(filled) };
        match source.read(free) {
            Ok(0) => return Ok(filled),
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
}

/// Reads and checks a cache file of at most `limit` bytes; a longer one is refused by
/// its length before any of it is read, and by the bytes read where its length says
/// less than it holds, as a device's, a pipe's or a growing file's can: no more than
/// `limit` + 1 bytes are read (ADR-0036). A read interrupted is tried again; another
/// error ends it as `CacheError::Io`.
pub fn read(path: &Path, limit: u64) -> Result<CacheFile, CacheError> {
    let file = std::fs::File::open(path)?;
    let length = file.metadata()?.len();
    if length > limit {
        return Err(CacheError::FileTooLarge { length, limit });
    }
    // Read into aligned blocks, so that the file is held once, not copied to align it:
    // room for the length it reports and a block more, grown should it hold more, as a
    // device, a pipe or a growing file may, up to the limit and one byte.
    let room = usize::try_from(length).unwrap_or(0).div_ceil(BLOCK) + 1;
    let mut blocks = vec![Block::new_zeroed(); room];
    let filled = fill(&mut file.take(limit.saturating_add(1)), &mut blocks)?;
    let read = u64::try_from(filled).unwrap_or(u64::MAX);
    if read > limit {
        return Err(CacheError::FileTooLarge { length: read, limit });
    }
    whole_blocks(read)?;
    blocks.truncate(filled / BLOCK);
    let index = index(blocks.as_bytes())?;
    Ok(CacheFile { storage: Storage::Copied(blocks), index })
}

/// Checks bytes whole as a cache file, copying them to an aligned buffer, as
/// docs/formats.md §3.4 states; of several broken rules, which is named is not
/// promised.
pub fn from_bytes(bytes: &[u8]) -> Result<CacheFile, CacheError> {
    whole_blocks(u64::try_from(bytes.len()).unwrap_or(u64::MAX))?;
    let blocks = aligned(bytes);
    let index = index(blocks.as_bytes())?;
    Ok(CacheFile { storage: Storage::Copied(blocks), index })
}

impl CacheFile {
    /// The file's format version: 2 (docs/formats.md §3).
    pub fn version(&self) -> u32 {
        self.index.version
    }

    /// The grids, in the file's order, numbered among themselves: an entry of bytes is not
    /// one.
    pub fn grids(&self) -> Vec<GridEntry<'_>> {
        let all = self.storage.bytes();
        self.index
            .grids
            .iter()
            .enumerate()
            .map(|(index, e)| GridEntry {
                index,
                meta: &e.meta,
                bounds: e.bounds,
                value_type: e.value_type,
                bytes: all.get(e.grid.clone()).unwrap_or_default(),
            })
            .collect()
    }

    /// The entries of bytes, in the file's order.
    pub fn bytes_entries(&self) -> Vec<BytesEntry<'_>> {
        let all = self.storage.bytes();
        self.index
            .bytes
            .iter()
            .map(|b| BytesEntry {
                name: &b.name,
                extra: &b.extra,
                bytes: all.get(b.range.clone()).unwrap_or_default(),
            })
            .collect()
    }
}

/// A grid a cache file holds (`CacheFile::grids`), checked whole when the file was read.
#[derive(Debug, Clone, Copy)]
pub struct GridEntry<'a> {
    index: usize,
    meta: &'a GridMeta,
    bounds: Bounds,
    value_type: ValueType,
    bytes: &'a [u8],
}

impl<'a> GridEntry<'a> {
    /// Its place among the file's grids, which `CacheError::Grid` names.
    pub fn index(&self) -> usize {
        self.index
    }

    /// Its metadata.
    pub fn meta(&self) -> &'a GridMeta {
        self.meta
    }

    /// The bounds the file states for it: the box of the voxels its leaves and tiles cover,
    /// checked against them (docs/formats.md §3.4).
    pub fn bounds(&self) -> Bounds {
        self.bounds
    }

    /// The type of its values.
    pub fn value_type(&self) -> ValueType {
        self.value_type
    }

    /// A view of its values of type `T`; another type is refused as `CacheError::Grid` with
    /// `FormatError::WrongValueType`.
    pub fn view<T: Stored>(&self) -> Result<GridView<'a, T>, CacheError> {
        GridView::new(self.bytes).map_err(|error| CacheError::Grid { index: self.index, error })
    }

    /// The bytes thawing it is counted to make (ARCHITECTURE.md §4.7): each leaf its values'
    /// bytes and 1024 more, each tile 256. Its nodes are not counted.
    pub fn thaw_size(&self) -> Result<u64, CacheError> {
        struct Size<'a>(GridEntry<'a>);
        impl ForType for Size<'_> {
            type Out = Result<u64, CacheError>;
            fn run<T: Stored>(self) -> Self::Out {
                let view = self.0.view::<T>()?;
                let values = u64::try_from(LEAF_VOXELS * size_of::<T>()).unwrap_or(u64::MAX);
                let count =
                    |n: usize, each: u64| u64::try_from(n).unwrap_or(u64::MAX).saturating_mul(each);
                Ok(count(view.leaf_count(), values.saturating_add(THAW_LEAF))
                    .saturating_add(count(view.tile_count(), THAW_TILE)))
            }
        }
        with_type(self.value_type, Size(*self))
    }

    /// It thawed into its topology and a field of the type the file states, refused as
    /// `CacheError::ThawTooLarge` if thawing it is counted (`thaw_size`) to make more than
    /// `limit` bytes.
    pub fn thaw(&self, limit: u64) -> Result<(Topology, AnyField), CacheError> {
        struct Thaw<'a>(GridEntry<'a>);
        impl ForType for Thaw<'_> {
            type Out = Result<(Topology, AnyField), CacheError>;
            fn run<T: Stored>(self) -> Self::Out {
                let (t, f) = self
                    .0
                    .view::<T>()?
                    .thaw()
                    .map_err(|error| CacheError::Grid { index: self.0.index, error })?;
                Ok((t, f.into()))
            }
        }
        let need = self.thaw_size()?;
        if need > limit {
            return Err(CacheError::ThawTooLarge { index: self.index, need, limit });
        }
        with_type(self.value_type, Thaw(*self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A source that gives `parts` in turn, each bytes or an error of a kind, then its end.
    struct Parts(Vec<Result<Vec<u8>, std::io::ErrorKind>>);

    impl Read for Parts {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.0.is_empty() {
                return Ok(0);
            }
            match self.0.remove(0) {
                Ok(b) => {
                    buf[..b.len()].copy_from_slice(&b);
                    Ok(b.len())
                }
                Err(kind) => Err(kind.into()),
            }
        }
    }

    /// `read`'s promise, held through the loop it reads with: a read
    /// interrupted is tried again, another error ends it.
    #[test]
    fn a_read_interrupted_is_tried_again_and_another_error_ends_it() {
        use std::io::ErrorKind::{Interrupted, Other};
        let mut blocks = vec![Block::new_zeroed(); 1];
        let mut source = Parts(vec![Ok(vec![1; 16]), Err(Interrupted), Ok(vec![2; 16])]);
        assert_eq!(fill(&mut source, &mut blocks).unwrap(), 32);
        assert_eq!(&blocks.as_bytes()[..32], [[1; 16], [2; 16]].concat());
        let mut source = Parts(vec![Ok(vec![1; 16]), Err(Other), Ok(vec![2; 16])]);
        assert_eq!(fill(&mut source, &mut blocks).map_err(|e| e.kind()), Err(Other));
    }
}
