//! The cache profile (ARCHITECTURE.md §4.7, ADR-0016): grids with their metadata in one
//! file, each grid's frozen buffer on a 32-byte boundary. This module parses untrusted
//! bytes: it is listed in `UNTRUSTED_INPUT`, indexes nothing (CODING.md Rule 3), checks the
//! whole file before it answers, and is fuzzed (TESTING.md T6).
//!
//! Format version 1's bytes are in `docs/formats.md` §3, which this code follows.

use std::io::Read;
use std::path::Path;

use zerocopy::{FromZeros, IntoBytes};

use crate::error::{CacheError, FormatError};
use crate::field::{Field, ForType, Stored, with_type};
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
    pub(crate) meta: GridMeta,
    pub(crate) bounds: Bounds,
    pub(crate) value_type: u32,
    pub(crate) frozen: FrozenGrid,
}

impl CacheEntry {
    /// Freezes a topology and a field on it, with its metadata, for a cache file. The
    /// metadata may hold no key beginning `hermite.`: those are a Hermite set's, written by
    /// `Hermite::entries`, so that a grid is not taken for one by mistake.
    pub fn new<T: Stored>(meta: &GridMeta, t: &Topology, f: &Field<T>) -> Result<Self, CacheError> {
        if let Some(key) = meta.extra.keys().find(|k| k.starts_with(crate::hermite::PREFIX)) {
            return Err(CacheError::ReservedKey(key.clone()));
        }
        Self::with_reserved_keys(meta, t, f)
    }

    /// As `new`, the metadata free to hold keys beginning `hermite.`: for a Hermite set's
    /// own grids, or a file made on purpose to break its format, as a test of a reader.
    pub fn with_reserved_keys<T: Stored>(
        meta: &GridMeta,
        t: &Topology,
        f: &Field<T>,
    ) -> Result<Self, CacheError> {
        meta.check()?;
        Ok(Self {
            meta: meta.clone(),
            bounds: Bounds::of(t)?,
            value_type: T::TYPE_CODE,
            frozen: freeze(t, f)?,
        })
    }
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
    put_u32(&mut out, u32_of(m.extra.len())?);
    for (k, v) in &m.extra {
        put_str(&mut out, k)?;
        put_str(&mut out, v)?;
    }
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

/// A grid as the cache file places it: its metadata's length in bytes, its frozen buffer's
/// in blocks, and its value type.
pub(crate) struct Part {
    pub(crate) meta_len: usize,
    pub(crate) grid_blocks: usize,
    pub(crate) value_type: u32,
}

/// The header and directory of a cache file of `parts`, and the block each grid's frozen
/// buffer starts at; each metadata starts at the block after the part before it.
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
            0,
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

/// Writes the file holding `entries` to `out`, part by part, so that no copy of the whole
/// is made.
fn emit(entries: &[CacheEntry], out: &mut impl std::io::Write) -> Result<(), CacheError> {
    let metas =
        entries.iter().map(|e| encode_meta(&e.meta, e.bounds)).collect::<Result<Vec<_>, _>>()?;
    let parts: Vec<_> = entries
        .iter()
        .zip(&metas)
        .map(|(e, m)| Part {
            meta_len: m.len(),
            grid_blocks: e.frozen.as_bytes().len() / BLOCK,
            value_type: e.value_type,
        })
        .collect();
    let (head, places) = head(&parts)?;
    out.write_all(&head)?;
    let mut written = head.len();
    // Each metadata starts where the part before it ends, the head or a grid, at a block.
    for ((e, m), grid_at) in entries.iter().zip(&metas).zip(places) {
        out.write_all(m)?;
        written += m.len();
        pad_to(out, grid_at * BLOCK, &mut written)?;
        out.write_all(e.frozen.as_bytes())?;
        written += e.frozen.as_bytes().len();
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
pub(crate) fn replace(
    path: &Path,
    emit: impl FnOnce(&mut std::io::BufWriter<std::fs::File>) -> Result<(), CacheError>,
) -> Result<(), CacheError> {
    use std::io::Write;
    let name = path.file_name().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "the path names no file")
    })?;
    let mut hidden = std::ffi::OsString::from(".");
    hidden.push(name);
    let n = WRITES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    hidden.push(format!(".{}.{n}.tmp", std::process::id()));
    let temp = path.with_file_name(hidden);
    let written = (|| -> Result<(), CacheError> {
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

/// One grid of a read file.
#[derive(Debug, Clone)]
pub(crate) struct Entry {
    meta: GridMeta,
    bounds: Bounds,
    value_type: u32,
    /// The frozen buffer's byte range in the file.
    pub(crate) grid: std::ops::Range<usize>,
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
    pub(crate) fn bytes(&self) -> &[u8] {
        match self {
            Storage::Copied(blocks) => blocks.as_bytes(),
            #[cfg(feature = "mmap")]
            Storage::Mapped(map) => map,
        }
    }
}

/// Checks a grid's frozen buffer with the type its directory names.
fn check_grid(bytes: &[u8], value_type: u32) -> Result<(), FormatError> {
    struct Check<'a>(&'a [u8]);
    impl ForType for Check<'_> {
        type Out = Result<(), FormatError>;
        fn run<T: Stored>(self) -> Self::Out {
            GridView::<T>::new(self.0).map(|_| ())
        }
    }
    with_type(value_type, Check(bytes))
        .unwrap_or_else(|found| Err(FormatError::UnknownValueType(found)))
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

pub(crate) fn zero(bytes: &[u8]) -> bool {
    bytes.iter().all(|b| *b == 0)
}

/// Reads `source` to its end into `blocks`, from their start, growing them as it needs:
/// the bytes read. A read interrupted is tried again.
fn fill(source: &mut impl Read, blocks: &mut Vec<Block>) -> std::io::Result<usize> {
    let mut filled = 0_usize;
    loop {
        if filled == blocks.len() * BLOCK {
            blocks.resize(blocks.len() * 2, Block::new_zeroed());
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

impl CacheFile {
    /// Reads and checks a cache file of at most `limit` bytes; a longer one is refused by
    /// its length before any of it is read, and by the bytes read where its length says
    /// less than it holds, as a device's, a pipe's or a growing file's can: no more than
    /// `limit` + 1 bytes are read (ADR-0036). A read interrupted is tried again; another
    /// error ends it as `CacheError::Io`.
    pub fn read(path: &Path, limit: u64) -> Result<Self, CacheError> {
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
        if !filled.is_multiple_of(BLOCK) {
            return Err(CacheError::Inconsistent("length"));
        }
        blocks.truncate(filled / BLOCK);
        let grids = index(blocks.as_bytes())?;
        Ok(Self { storage: Storage::Copied(blocks), grids })
    }

    /// Checks bytes whole as a cache file, copying them to an aligned buffer, as
    /// docs/formats.md §3.4 states; of several broken rules, which is named is not
    /// promised.
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

    /// The bounds the file states for a grid: its writer's box of the voxels its leaves and
    /// tiles cover, not checked against the grid (docs/formats.md §3.4), so a file from
    /// another writer may state others. `Bounds::of` on the grid's thawed topology gives
    /// the true ones.
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

    /// `CacheFile::read`'s promise, held through the loop it reads with: a read
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
