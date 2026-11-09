//! The archive profile (ARCHITECTURE.md §4.7, ADR-0016): grids with their metadata in one
//! file, each frozen buffer cut into chunks compressed apart by `lz4_flex`'s block format,
//! read back as the cache file of the same grids. This module parses untrusted bytes: it is
//! listed in `UNTRUSTED_INPUT`, indexes nothing (CODING.md Rule 3), checks the whole file
//! before it answers, and is fuzzed (TESTING.md T6).
//!
//! The bytes are in `docs/formats.md` §8, which this code follows: version 2, which holds
//! entries of bytes beside grids (ADR-0051); version 1 is neither read nor written
//! (ADR-0054). A region's read (§8.6) checks the archive's structure and only the chunks
//! it decompresses; a listing (§8.7) checks the structure and every entry's metadata, and
//! decompresses nothing.

use std::collections::BTreeMap;
use std::path::Path;

use rayon::prelude::*;
use zerocopy::{FromBytes, FromZeros, IntoBytes};

use crate::cache::{
    CacheEntry, CacheFile, Cursor, KIND_BYTES, KIND_GRID, Part, Payload, Storage,
    decode_bytes_meta, decode_meta, head, index, pad_to, put_u32, range, read_within, u32_of,
    within_blocks, zero,
};
use crate::error::{CacheError, FormatError};
use crate::field::local_index;
use crate::field::{ForType, LEAF_VOXELS, Stored, ValueType, with_type};
use crate::frozen::{BLOCK, Block, Header};
use crate::meta::{Bounds, GridMeta};
use crate::quantise::{self, Kind, Quantised};
use crate::tree::Coord;
use crate::view::{GridView, ViewLocation};

use crate::kind::ARCHIVE_MAGIC as MAGIC;
/// The format version this build reads and writes.
const VERSION: u32 = 2;
/// Blocks a chunk of an entry of bytes holds, 512 × 512 bytes, but its last (§8.3).
const BYTES_CHUNK: usize = 512 * 512 / BLOCK;
/// The LZ4 block format, the only codec (ADR-0016).
const CODEC: u32 = 1;
/// Leaves a chunk (ADR-0016).
const CHUNK_LEAVES: usize = 512;
/// Bytes a chunk table entry.
const ENTRY: usize = 8;

/// How a grid's leaf values are stored in an archive (docs/formats.md §8.5, ADR-0016). The
/// background and the tiles are stored exact whatever the encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Encoding {
    /// As they are, read back bit for bit.
    #[default]
    Exact,
    /// Each scalar as the IEEE 754 binary16 nearest it, ties to even, byte-shuffled: read
    /// back within 2⁻¹¹ × |x| + 2⁻²⁵ of x. For floating value types only; a NaN or a value
    /// beyond ±65504 is refused.
    Binary16,
    /// Each scalar as 8 bits over its component's range across the grid's leaf values:
    /// read back within a 510th of the range, to rounding. For floating value types only; a
    /// NaN, an infinity or a range wider than an `f64` holds is refused.
    EightBit,
}

impl Encoding {
    /// Its code in a directory entry.
    fn code(self) -> u32 {
        match self {
            Self::Exact => 0,
            Self::Binary16 => 1,
            Self::EightBit => 2,
        }
    }
}

/// A chunk: its length in blocks of the frozen buffer, and whether it holds leaf values.
type Piece = (usize, bool);

/// Each chunk of a frozen buffer, as docs/formats.md §8.3 cuts it: at its leaf values'
/// start, every `CHUNK_LEAVES` leaves within them and at their end, leaving out a piece of
/// no bytes. The buffer is one `GridView::new` accepts.
fn cuts(frozen: &[u8]) -> Result<Vec<Piece>, CacheError> {
    cut(frozen, frozen.len() / BLOCK)
}

/// The chunks of a frozen buffer of `total` blocks whose header begins `head`.
fn cut(head: &[u8], total: usize) -> Result<Vec<Piece>, CacheError> {
    let bad = || CacheError::Inconsistent("grid");
    let (h, _) = Header::ref_from_prefix(head).map_err(|_| bad())?;
    let [_, _, _, leaves, _] = h.counts.map(|c| usize::try_from(c.get()).unwrap_or(usize::MAX));
    let [_, _, _, start, _] = h.offsets.map(|c| usize::try_from(c.get()).unwrap_or(usize::MAX));
    let leaf = usize::try_from(h.value_size.get())
        .map_err(|_| bad())?
        .checked_mul(512 / BLOCK)
        .ok_or_else(bad)?;
    let chunk = leaf.checked_mul(CHUNK_LEAVES).ok_or_else(bad)?;
    let end = leaves.checked_mul(leaf).and_then(|n| n.checked_add(start)).ok_or_else(bad)?;
    if end > total || chunk == 0 {
        return Err(bad());
    }
    let mut marks = vec![0];
    marks.extend((start..end).step_by(chunk));
    marks.extend([end, total]);
    Ok(marks
        .windows(2)
        .filter_map(|w| match w {
            [a, b] if b > a => Some((b - a, *a >= start && *b <= end)),
            _ => None,
        })
        .collect())
}

/// An entry as written: its metadata and place, its encoding's code and ranges' bytes, its
/// chunks' lengths in blocks, and its chunks compressed.
struct Packed {
    meta: Vec<u8>,
    part: Part,
    encoding: u32,
    ranges: Vec<u8>,
    raw: Vec<usize>,
    chunks: Vec<Vec<u8>>,
}

/// How grid `index`, of `pieces`, is quantised by `encoding`, if it is.
fn plan(
    index: usize,
    value_type: u32,
    encoding: Encoding,
    pieces: &[(&[u8], bool)],
) -> Result<Option<(Kind, Quantised)>, CacheError> {
    let eight = match encoding {
        Encoding::Exact => return Ok(None),
        Encoding::Binary16 => false,
        Encoding::EightBit => true,
    };
    let kind = Kind::of(value_type)
        .ok_or(CacheError::NotQuantised { index, value_type: ValueType::from_code(value_type) })?;
    let q = if eight {
        let leaves = pieces.iter().filter(|(_, leaf)| *leaf).map(|(p, _)| *p);
        Quantised::EightBit(
            quantise::ranges(kind, leaves).ok_or(CacheError::Unrepresentable(index))?,
        )
    } else {
        Quantised::Binary16
    };
    Ok(Some((kind, q)))
}

/// Entry `index`'s chunks: a grid's frozen buffer cut by `cut` and
/// its leaf values encoded by `encoding`; an entry of bytes cut every `BYTES_CHUNK` blocks,
/// exact.
fn pack(index: usize, e: &CacheEntry, encoding: Encoding, cut: Cut) -> Result<Packed, CacheError> {
    let (meta, part) = e.part()?;
    let (bytes, value_type) = match &e.payload {
        Payload::Grid { frozen, value_type, .. } => (frozen.as_bytes(), *value_type),
        Payload::Bytes { .. } if encoding != Encoding::Exact => {
            return Err(CacheError::NotQuantised { index, value_type: None });
        }
        Payload::Bytes { bytes, .. } => {
            let mut padded = bytes.clone();
            padded.resize(bytes.len().next_multiple_of(BLOCK), 0);
            let blocks = padded.len() / BLOCK;
            let raw: Vec<Piece> = (0..blocks)
                .step_by(BYTES_CHUNK)
                .map(|at| ((blocks - at).min(BYTES_CHUNK), false))
                .collect();
            return compress(meta, part, &padded, &raw, None, 0, index);
        }
    };
    let raw = cut(bytes)?;
    let mut pieces = Vec::new();
    let mut rest = bytes;
    for (r, leaf) in &raw {
        let (piece, after) =
            rest.split_at_checked(r * BLOCK).ok_or(CacheError::Inconsistent("grid"))?;
        pieces.push((piece, *leaf));
        rest = after;
    }
    let values = plan(index, value_type, encoding, &pieces)?;
    compress(meta, part, bytes, &raw, values, encoding.code(), index)
}

/// The chunks of `bytes`, cut into `raw`, encoded by `values` where they hold leaf values,
/// each compressed: in parallel, collected in order, the same bytes whatever the threads.
fn compress(
    meta: Vec<u8>,
    part: Part,
    bytes: &[u8],
    raw: &[Piece],
    values: Option<(Kind, Quantised)>,
    encoding: u32,
    index: usize,
) -> Result<Packed, CacheError> {
    let mut pieces = Vec::new();
    let mut rest = bytes;
    for (r, leaf) in raw {
        let (piece, after) =
            rest.split_at_checked(r * BLOCK).ok_or(CacheError::Inconsistent("grid"))?;
        pieces.push((piece, *leaf));
        rest = after;
    }
    let chunks = pieces
        .par_iter()
        .map(|(p, leaf)| match &values {
            Some((kind, q)) if *leaf => {
                quantise::encode(q, *kind, p).map(|b| lz4_flex::block::compress(&b))
            }
            Some(_) | None => Some(lz4_flex::block::compress(p)),
        })
        .collect::<Option<Vec<_>>>()
        .ok_or(CacheError::Unrepresentable(index))?;
    let ranges = match &values {
        Some((_, Quantised::EightBit(r))) => quantise::range_bytes(r),
        Some((_, Quantised::Binary16)) | None => Vec::new(),
    };
    let raw = raw.iter().map(|(r, _)| *r).collect();
    Ok(Packed { meta, part, encoding, ranges, raw, chunks })
}

/// How a frozen buffer is cut into chunks: `cuts`, but for a test of the reader.
type Cut = fn(&[u8]) -> Result<Vec<Piece>, CacheError>;

/// Writes the archive holding `grids` to `out`, part by part, each frozen buffer cut by
/// `cut`.
fn emit(
    grids: &[(&CacheEntry, Encoding)],
    cut: Cut,
    out: &mut impl std::io::Write,
) -> Result<(), CacheError> {
    let packed = grids
        .iter()
        .enumerate()
        .map(|(i, (e, encoding))| pack(i, e, *encoding, cut))
        .collect::<Result<Vec<_>, _>>()?;
    let mut top = Vec::new();
    for x in [MAGIC, VERSION, u32_of(grids.len())?, CODEC, u32_of(CHUNK_LEAVES)?, 0, 0, 0] {
        put_u32(&mut top, x);
    }
    let mut at = 1 + grids.len();
    for p in &packed {
        let table_at = at + p.meta.len().div_ceil(BLOCK);
        let first = table_at + (p.chunks.len() * ENTRY + p.ranges.len()).div_ceil(BLOCK);
        at = first + p.chunks.iter().map(|c| c.len().div_ceil(BLOCK)).sum::<usize>();
        for x in [
            u32_of(table_at - p.meta.len().div_ceil(BLOCK))?,
            u32_of(p.meta.len())?,
            u32_of(table_at)?,
            u32_of(p.chunks.len())?,
            u32_of(p.part.grid_blocks)?,
            p.part.value_type,
            p.encoding,
            p.part.kind,
        ] {
            put_u32(&mut top, x);
        }
    }
    u32_of(at)?;
    out.write_all(&top)?;
    let mut written = top.len();
    for p in &packed {
        let mut table = Vec::new();
        for (r, c) in p.raw.iter().zip(&p.chunks) {
            put_u32(&mut table, u32_of(*r)?);
            put_u32(&mut table, u32_of(c.len())?);
        }
        table.extend_from_slice(&p.ranges);
        for part in std::iter::once(&p.meta).chain([&table]).chain(&p.chunks) {
            out.write_all(part)?;
            written += part.len();
            pad_to(out, written.next_multiple_of(BLOCK), &mut written)?;
        }
    }
    Ok(())
}

/// Every grid of `entries`, exact.
fn exact(entries: &[CacheEntry]) -> Vec<(&CacheEntry, Encoding)> {
    entries.iter().map(|e| (e, Encoding::Exact)).collect()
}

/// The bytes of an archive holding `entries`, in order, their values exact.
pub fn to_bytes(entries: &[CacheEntry]) -> Result<Vec<u8>, CacheError> {
    to_bytes_with(&exact(entries))
}

/// The bytes of an archive holding `grids`, in order, each grid's leaf values stored by its
/// encoding. A grid quantised but not of a floating value type is refused, `NotQuantised`,
/// and one holding a value its encoding cannot hold, `Unrepresentable`, both naming its
/// place in `grids`. An entry of bytes is stored exact; another encoding for one is
/// refused, `NotQuantised`.
pub fn to_bytes_with(grids: &[(&CacheEntry, Encoding)]) -> Result<Vec<u8>, CacheError> {
    let mut out = Vec::new();
    emit(grids, cuts, &mut out)?;
    Ok(out)
}

/// Writes an archive holding `entries`, their values exact, replacing `path` whole as
/// `cache::write` does.
pub fn write(path: &Path, entries: &[CacheEntry]) -> Result<(), CacheError> {
    write_with(path, &exact(entries))
}

/// Writes an archive holding `grids`, each by its encoding, as `to_bytes_with` makes it,
/// replacing `path` whole as `cache::write` does.
pub fn write_with(path: &Path, grids: &[(&CacheEntry, Encoding)]) -> Result<(), CacheError> {
    crate::cache::replace(path, |out| emit(grids, cuts, out))
}

/// An entry as read: where its metadata is, its payload's length in blocks, its value
/// type and kind, how its leaf values are quantised, if they are, and its chunks with their
/// lengths in blocks.
struct Found<'a> {
    meta: &'a [u8],
    grid_blocks: usize,
    value_type: u32,
    kind: u32,
    values: Option<(Kind, Quantised)>,
    chunks: Vec<(&'a [u8], usize)>,
}

impl Found<'_> {
    /// How the grid's leaf values are encoded.
    fn encoding(&self) -> Encoding {
        match self.values {
            None => Encoding::Exact,
            Some((_, Quantised::Binary16)) => Encoding::Binary16,
            Some((_, Quantised::EightBit(_))) => Encoding::EightBit,
        }
    }
}

/// Reads the header block: the magic number, a format version this build reads, codec and
/// chunk size, and unused words of zero. Returns the entry count.
fn header(c: &mut Cursor<'_>) -> Result<usize, CacheError> {
    let magic = c.u32()?;
    if magic != MAGIC {
        return Err(CacheError::BadMagic(magic));
    }
    let version = c.u32()?;
    if version != VERSION {
        return Err(CacheError::UnsupportedVersion(version));
    }
    let count = c.u32()?;
    if [c.u32()?, c.u32()?] != [CODEC, u32_of(CHUNK_LEAVES)?] || !zero(c.take(BLOCK - 20)?) {
        return Err(CacheError::Inconsistent("header"));
    }
    usize::try_from(count).map_err(|_| CacheError::TooLarge)
}

/// Checks an archive's parts, each where the writer puts it with padding of zeros between
/// and nothing after the last: every entry's metadata, chunk table and chunks.
fn find(all: &[u8]) -> Result<Vec<Found<'_>>, CacheError> {
    if !all.len().is_multiple_of(BLOCK) {
        return Err(CacheError::Inconsistent("length"));
    }
    let mut c = Cursor { rest: all, what: "header" };
    let count = header(&mut c)?;
    c.what = "directory";
    let mut after =
        count.checked_add(1).and_then(|n| n.checked_mul(BLOCK)).ok_or(CacheError::TooLarge)?;
    if after > all.len() {
        return Err(CacheError::Truncated("directory"));
    }
    let mut grids = Vec::new();
    for _ in 0..count {
        let (g, end) = grid(all, &mut c, after)?;
        grids.push(g);
        after = end;
    }
    if after != all.len() {
        return Err(CacheError::Inconsistent("end"));
    }
    Ok(grids)
}

fn size(n: u32) -> Result<usize, CacheError> {
    usize::try_from(n).map_err(|_| CacheError::TooLarge)
}

/// Checks one grid's directory entry, read from `c`, and its parts, the first at byte
/// `after`: the grid and the byte after its last part's padding.
fn grid<'a>(
    all: &'a [u8],
    c: &mut Cursor<'_>,
    after: usize,
) -> Result<(Found<'a>, usize), CacheError> {
    let [meta_at, meta_len, table_at, chunk_count, grid_blocks, value_type, encoding, entry] =
        [(); 8].map(|()| c.u32());
    let (entry, value_type, encoding) = (entry?, value_type?, encoding?);
    let bytes_ok = value_type == 0 && encoding == 0;
    if !(entry == KIND_GRID || (entry == KIND_BYTES && bytes_ok)) {
        return Err(CacheError::Inconsistent("directory"));
    }
    let (kind, components) =
        if entry == KIND_BYTES { (None, 0) } else { encoding_of(value_type, encoding)? };
    let meta = range(meta_at?, size(meta_len?)?, after, all.len(), "metadata")?;
    let entries = size(chunk_count?)?.checked_mul(ENTRY).ok_or(CacheError::TooLarge)?;
    let table_len =
        entries.checked_add(components * quantise::RANGE).ok_or(CacheError::TooLarge)?;
    let table_start = meta.end.next_multiple_of(BLOCK);
    let table = range(table_at?, table_len, table_start, all.len(), "chunk table")?;
    let what = "chunk table";
    let (listed, tail) = all
        .get(table.clone())
        .and_then(|t| t.split_at_checked(entries))
        .ok_or(CacheError::Truncated(what))?;
    let values = match kind {
        Some(k) if components > 0 => {
            let r =
                quantise::read_ranges(tail, components).ok_or(CacheError::Inconsistent(what))?;
            Some((k, Quantised::EightBit(r)))
        }
        Some(k) => Some((k, Quantised::Binary16)),
        None => None,
    };
    let mut parts = vec![meta.clone(), table];
    let chunks = chunks(all, listed, &mut parts)?;
    let grid_blocks = size(grid_blocks?)?;
    let sum = chunks.iter().try_fold(0_usize, |s, (_, raw)| s.checked_add(*raw));
    if sum != Some(grid_blocks) {
        return Err(CacheError::Inconsistent(what));
    }
    let end = parts.last().map_or(after, |p| p.end).next_multiple_of(BLOCK);
    parts.push(end..end);
    for w in parts.windows(2) {
        if let [a, b] = w
            && !zero(all.get(a.end..b.start).ok_or(CacheError::Truncated("padding"))?)
        {
            return Err(CacheError::Inconsistent("padding"));
        }
    }
    let meta = all.get(meta).ok_or(CacheError::Truncated("metadata"))?;
    Ok((Found { meta, grid_blocks, value_type, kind: entry, values, chunks }, end))
}

/// A grid's value encoding `code`, checked against its value type: the kind of a
/// quantised grid's values, and the ranges its table holds, a component's for 8 bits.
fn encoding_of(value_type: u32, code: u32) -> Result<(Option<Kind>, usize), CacheError> {
    let quantised = || Kind::of(value_type).ok_or(CacheError::Inconsistent("directory"));
    match code {
        0 => Ok((None, 0)),
        1 => Ok((Some(quantised()?), 0)),
        2 => quantised().map(|k| (Some(k), k.components())),
        code => Err(CacheError::UnknownEncoding(code)),
    }
}

/// The chunks a grid's table entries `listed` list, each with its length in blocks, none
/// zero, each at the block after the part before it; their places are added to `parts`.
fn chunks<'a>(
    all: &'a [u8],
    listed: &[u8],
    parts: &mut Vec<std::ops::Range<usize>>,
) -> Result<Vec<(&'a [u8], usize)>, CacheError> {
    let what = "chunk table";
    let mut t = Cursor { rest: listed, what };
    let mut chunks = Vec::new();
    while !t.rest.is_empty() {
        let (raw, len) = (size(t.u32()?)?, size(t.u32()?)?);
        if raw == 0 || raw.checked_mul(BLOCK).is_none() {
            return Err(CacheError::Inconsistent(what));
        }
        let start = parts.last().map_or(0, |p| p.end.next_multiple_of(BLOCK));
        let end = start.checked_add(len).ok_or(CacheError::TooLarge)?;
        chunks.push((all.get(start..end).ok_or(CacheError::Truncated("chunk"))?, raw));
        parts.push(start..end);
    }
    Ok(chunks)
}

/// Checks an archive's bytes whole, as docs/formats.md §8.4 states, and decompresses it
/// into the cache file of the same grids, of at most `limit` bytes: a larger one is refused
/// by the lengths the archive states, before any of it is made. Of several broken rules,
/// which is named is not promised.
pub fn from_bytes(bytes: &[u8], limit: u64) -> Result<CacheFile, CacheError> {
    let found = find(bytes)?;
    let parts: Vec<_> = found
        .iter()
        .map(|g| Part {
            meta_len: g.meta.len(),
            grid_blocks: g.grid_blocks,
            value_type: g.value_type,
            kind: g.kind,
        })
        .collect();
    let (top, places) = head(&parts)?;
    let blocks = places
        .last()
        .zip(found.last())
        .map_or(Some(top.len() / BLOCK), |(at, g)| at.checked_add(g.grid_blocks))
        .ok_or(CacheError::TooLarge)?;
    let length = u64::try_from(blocks)
        .ok()
        .and_then(|b| b.checked_mul(BLOCK as u64))
        .ok_or(CacheError::TooLarge)?;
    if length > limit {
        return Err(CacheError::FileTooLarge { length, limit });
    }
    let out = unpack(&found, &top, blocks)?;
    let mut index = index(out.as_bytes())?;
    let grids = found.iter().filter(|g| g.kind == KIND_GRID);
    for (e, g) in index.grids.iter_mut().zip(grids) {
        e.encoding = g.encoding();
    }
    Ok(CacheFile { storage: Storage::Copied(out), index })
}

/// A chunk to decompress: its grid's place and its own, its bytes, the room it fills, and
/// whether it holds leaf values.
struct Work<'a, 'b> {
    grid: usize,
    chunk: usize,
    bytes: &'a [u8],
    room: &'b mut [u8],
    leaf: bool,
}

/// The cache file of `blocks` blocks that `found` decompresses into, below its header and
/// directory `top`: each grid's metadata, then its chunks. Every grid's first chunk is
/// decompressed first, in parallel, for the frozen header that tells how the grid is cut
/// (docs/formats.md §8.4, rule 6); then the others, in parallel, leaf values restored.
fn unpack(found: &[Found<'_>], top: &[u8], blocks: usize) -> Result<Vec<Block>, CacheError> {
    let mut out = vec![Block::new_zeroed(); blocks];
    let mut work = rooms(found, top, out.as_mut_bytes())?;
    run(found, work.iter_mut().filter(|w| w.chunk == 0).collect())?;
    for (i, g) in found.iter().enumerate() {
        let head = work.iter().find(|w| w.grid == i).map_or(&[][..], |w| &*w.room);
        let pieces = pieces(number(found, i), g, head)?;
        for (w, (_, leaf)) in work.iter_mut().filter(|w| w.grid == i).zip(pieces) {
            w.leaf = leaf;
        }
    }
    run(found, work.iter_mut().filter(|w| w.chunk != 0).collect())?;
    Ok(out)
}

/// The chunks of entry `index`, numbered among its kind's entries, as §8.3 cuts a grid's
/// frozen buffer, read from the frozen header in `head`, its first chunk decompressed, or
/// as §8.3 cuts an entry of bytes; a table that lists others is refused (§8.4, rule 6).
fn pieces(index: usize, g: &Found<'_>, head: &[u8]) -> Result<Vec<Piece>, CacheError> {
    let pieces = if g.kind == KIND_BYTES {
        let blocks = g.grid_blocks;
        (0..blocks).step_by(BYTES_CHUNK).map(|at| ((blocks - at).min(BYTES_CHUNK), false)).collect()
    } else {
        cut(head, g.grid_blocks)?
    };
    let first_leaf = pieces.first().is_some_and(|(_, leaf)| *leaf);
    if first_leaf || pieces.iter().map(|(r, _)| *r).ne(g.chunks.iter().map(|(_, raw)| *raw)) {
        return Err(if g.kind == KIND_BYTES {
            CacheError::BytesChunks(index)
        } else {
            CacheError::Chunks(index)
        });
    }
    Ok(pieces)
}

/// `out` split into the header and directory `top`, copied in, then each grid's metadata,
/// copied in, and the room each of its chunks fills, in order.
fn rooms<'a, 'b>(
    found: &[Found<'a>],
    top: &[u8],
    out: &'b mut [u8],
) -> Result<Vec<Work<'a, 'b>>, CacheError> {
    let (lead, mut rest) = out.split_at_mut_checked(top.len()).ok_or(CacheError::TooLarge)?;
    lead.copy_from_slice(top);
    let mut work = Vec::new();
    for (grid, g) in found.iter().enumerate() {
        let (meta, after) = rest.split_at_mut_checked(g.meta.len()).ok_or(CacheError::TooLarge)?;
        meta.copy_from_slice(g.meta);
        let pad = g.meta.len().next_multiple_of(BLOCK) - g.meta.len();
        let (_, mut after) = after.split_at_mut_checked(pad).ok_or(CacheError::TooLarge)?;
        for (chunk, (bytes, raw)) in g.chunks.iter().enumerate() {
            let (room, next) =
                after.split_at_mut_checked(raw * BLOCK).ok_or(CacheError::TooLarge)?;
            work.push(Work { grid, chunk, bytes, room, leaf: false });
            after = next;
        }
        rest = after;
    }
    Ok(work)
}

/// Decompresses `work` in parallel, a leaf chunk of a quantised grid through a scratch
/// buffer and restored; the first chunk in order that fails is named, whatever the threads.
fn run(found: &[Found<'_>], mut work: Vec<&mut Work<'_, '_>>) -> Result<(), CacheError> {
    let failed: Vec<_> = work
        .par_iter_mut()
        .map_init(Vec::new, |scratch, w| {
            let values = found.get(w.grid).and_then(|g| g.values.as_ref()).filter(|_| w.leaf);
            let done = match values {
                Some((kind, q)) => q.encoded_len(*kind, w.room.len()).is_some_and(|n| {
                    scratch.resize(n, 0);
                    let filled = lz4_flex::block::decompress_into(w.bytes, scratch).ok();
                    filled == Some(n) && quantise::restore(q, *kind, scratch, w.room)
                }),
                None => {
                    lz4_flex::block::decompress_into(w.bytes, w.room).ok() == Some(w.room.len())
                }
            };
            (!done).then_some((w.grid, w.chunk))
        })
        .collect();
    match failed.into_iter().flatten().next() {
        Some((place, chunk)) => {
            let index = number(found, place);
            Err(if found.get(place).is_some_and(|g| g.kind == KIND_BYTES) {
                CacheError::BytesDecompress { index, chunk }
            } else {
                CacheError::Decompress { index, chunk }
            })
        }
        None => Ok(()),
    }
}

/// Reads and checks an archive of at most `limit` bytes, decompressed into a cache file of
/// at most `limit` bytes: a longer file is refused by its length before any of it is read,
/// or by the bytes read where its length says less than it holds, and a larger cache file
/// by the lengths the archive states, before it is made (ADR-0036).
pub fn read(path: &Path, limit: u64) -> Result<CacheFile, CacheError> {
    from_bytes(&load(path, limit)?, limit)
}

/// An archive's bytes, of at most `limit`, refused by its length before any of it is read,
/// or by the bytes read where its length says less than it holds.
fn load(path: &Path, limit: u64) -> Result<Vec<u8>, CacheError> {
    read_within(path, limit, |length, limit| CacheError::FileTooLarge { length, limit })
}

/// What an archive holds, read from its directory and metadata without decompressing a
/// chunk (`list`, docs/formats.md §8.7): its grids and its entries of bytes, each in the
/// file's order.
#[derive(Debug, Clone)]
pub struct Listing {
    grids: Vec<ListedGrid>,
    bytes: Vec<ListedBytes>,
}

impl Listing {
    /// The grids, in the file's order, numbered among themselves as `GridEntry::index`
    /// numbers them: an entry of bytes is not one.
    pub fn grids(&self) -> &[ListedGrid] {
        &self.grids
    }

    /// The grids named `name`, every one in the file's order; none if no grid is.
    pub fn grids_named(&self, name: &str) -> Vec<&ListedGrid> {
        self.grids.iter().filter(|g| g.meta.name == name).collect()
    }

    /// The entries of bytes, in the file's order.
    pub fn bytes_entries(&self) -> &[ListedBytes] {
        &self.bytes
    }
}

/// A grid an archive lists: what its directory entry and metadata state, its values not
/// read. Its region, or the whole grid as the region of its bounds, is read by
/// `read_region` with its `index`.
#[derive(Debug, Clone)]
pub struct ListedGrid {
    index: usize,
    meta: GridMeta,
    bounds: Bounds,
    value_type: ValueType,
    encoding: Encoding,
}

impl ListedGrid {
    /// Its place among the file's grids, as `GridEntry::index` numbers it.
    pub fn index(&self) -> usize {
        self.index
    }

    /// Its metadata.
    pub fn meta(&self) -> &GridMeta {
        &self.meta
    }

    /// The bounds the file states for it, not checked against its leaves and tiles, which
    /// a listing does not read (docs/formats.md §8.7).
    pub fn bounds(&self) -> Bounds {
        self.bounds
    }

    /// The type of its values.
    pub fn value_type(&self) -> ValueType {
        self.value_type
    }

    /// How its leaf values are encoded.
    pub fn encoding(&self) -> Encoding {
        self.encoding
    }
}

/// An entry of bytes an archive lists: its name, its free pairs and its length, its bytes
/// not read.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ListedBytes {
    /// Its name.
    pub name: String,
    /// Its free pairs.
    pub extra: BTreeMap<String, String>,
    /// Its length in bytes.
    pub len: u64,
}

/// Lists an archive of at most `limit` bytes, as `list_from_bytes` does: a longer file is
/// refused by its length before any of it is read, or by the bytes read where its length
/// says less than it holds. The file is read whole; no chunk is decompressed.
pub fn list(path: &Path, limit: u64) -> Result<Listing, CacheError> {
    list_from_bytes(&load(path, limit)?)
}

/// Checks an archive's structure and every entry's metadata as docs/formats.md §8.7
/// states and lists its grids and entries of bytes, decompressing no chunk: a file whose
/// chunks a whole read refuses may be listed. Of several broken rules, which is named is
/// not promised.
pub fn list_from_bytes(bytes: &[u8]) -> Result<Listing, CacheError> {
    let mut listing = Listing { grids: Vec::new(), bytes: Vec::new() };
    for g in &find(bytes)? {
        if g.kind == KIND_BYTES {
            let (name, len, extra) = decode_bytes_meta(g.meta)?;
            within_blocks(len, g.grid_blocks)?;
            listing.bytes.push(ListedBytes { name, extra, len });
            continue;
        }
        let index = listing.grids.len();
        let (meta, bounds) = decode_meta(g.meta)?;
        let value_type = ValueType::from_code(g.value_type).ok_or(CacheError::Grid {
            index,
            error: FormatError::UnknownValueType(g.value_type),
        })?;
        let encoding = g.encoding();
        listing.grids.push(ListedGrid { index, meta, bounds, value_type, encoding });
    }
    Ok(listing)
}

/// One grid's region of an archive, a box of voxels, read without the leaf chunks it does
/// not need (docs/formats.md §8.6, ADR-0016): the grid's first chunk, its tile chunk, and
/// each leaf chunk holding a leaf that meets the box, each decompressed into a buffer of
/// its own. Its view answers within the box (`Region::view`).
#[derive(Debug)]
pub struct Region {
    index: usize,
    meta: GridMeta,
    bounds: Bounds,
    /// The type of the grid's values, and how they are encoded.
    values: (ValueType, Encoding),
    extent: Bounds,
    /// The frozen buffer's length in bytes.
    len: usize,
    head: Vec<Block>,
    tail: Vec<Block>,
    /// Each leaf chunk held, by its place among the grid's leaf chunks.
    chunks: Vec<(usize, Vec<Block>)>,
    /// The number and origin of each leaf meeting the box, in number order.
    leaves: Vec<(u32, Coord)>,
}

/// Reads grid `index`'s region `extent` from an archive of at most `limit` bytes, holding at
/// most `limit` bytes decompressed, both refused before they are read or made, as `read`
/// refuses them (ADR-0036). The file is read whole; a chunk not decompressed is not checked
/// (docs/formats.md §8.6).
pub fn read_region(
    path: &Path,
    index: usize,
    extent: Bounds,
    limit: u64,
) -> Result<Region, CacheError> {
    region_from_bytes(&load(path, limit)?, index, extent, limit)
}

/// Checks an archive's bytes as docs/formats.md §8.6 states and reads grid `index`'s region
/// `extent`, holding at most `limit` bytes decompressed: more is refused by the lengths the
/// archive states, before it is made. Of several broken rules, which is named is not
/// promised.
pub fn region_from_bytes(
    bytes: &[u8],
    index: usize,
    extent: Bounds,
    limit: u64,
) -> Result<Region, CacheError> {
    let found = find(bytes)?;
    let (place, g) = grid_number(&found, index)?;
    let (meta, bounds) = decode_meta(g.meta)?;
    let chunk = |k: usize| g.chunks.get(k).copied().ok_or(CacheError::Chunks(index));
    let (first, raw) = chunk(0)?;
    within(raw, limit)?;
    let head = decompress(&found, place, 0, first, raw, false)?;
    let pieces = pieces(index, g, head.as_bytes())?;
    let last = pieces.len().checked_sub(1).filter(|k| *k > 0);
    let tail = match last.filter(|k| pieces.get(*k).is_some_and(|(_, leaf)| !leaf)) {
        Some(k) => {
            let (bytes, raw) = chunk(k)?;
            within(head.len().saturating_add(raw), limit)?;
            decompress(&found, place, k, bytes, raw, false)?
        }
        None => Vec::new(),
    };
    let len = g.grid_blocks.checked_mul(BLOCK).ok_or(CacheError::TooLarge)?;
    let refused = |error| CacheError::Grid { index, error };
    let value_type = ValueType::from_code(g.value_type)
        .ok_or(refused(FormatError::UnknownValueType(g.value_type)))?;
    let leaves =
        meeting(value_type, head.as_bytes(), tail.as_bytes(), len, extent).map_err(refused)?;
    let mut wanted: Vec<usize> = leaves.iter().map(|(n, _)| *n as usize / CHUNK_LEAVES).collect();
    wanted.dedup();
    let mut sizes = Vec::new();
    let mut held = head.len().saturating_add(tail.len());
    for k in wanted {
        let (_, raw) = chunk(k.checked_add(1).ok_or(CacheError::TooLarge)?)?;
        held = held.saturating_add(raw);
        sizes.push((k, raw));
    }
    within(held, limit)?;
    let mut chunks: Vec<_> =
        sizes.into_iter().map(|(k, raw)| (k, vec![Block::new_zeroed(); raw])).collect();
    let mut work = Vec::new();
    for (k, room) in &mut chunks {
        let (bytes, _) = chunk(*k + 1)?;
        work.push(Work {
            grid: place,
            chunk: *k + 1,
            bytes,
            room: room.as_mut_bytes(),
            leaf: true,
        });
    }
    run(&found, work.iter_mut().collect())?;
    let values = (value_type, g.encoding());
    Ok(Region { index, meta, bounds, values, extent, len, head, tail, chunks, leaves })
}

/// The entry at `place` in the directory numbered among the entries of its kind, as
/// `GridEntry::index` numbers a grid (ADR-0057).
fn number(found: &[Found<'_>], place: usize) -> usize {
    let kind = found.get(place).map(|g| g.kind);
    found.iter().take(place).filter(|g| Some(g.kind) == kind).count()
}

/// Grid `index`, numbered among the grids alone, and its place in the directory.
fn grid_number<'f, 'a>(
    found: &'f [Found<'a>],
    index: usize,
) -> Result<(usize, &'f Found<'a>), CacheError> {
    found
        .iter()
        .enumerate()
        .filter(|(_, g)| g.kind == KIND_GRID)
        .nth(index)
        .ok_or(CacheError::NoGrid(index))
}

/// Refuses `blocks` blocks held beyond `limit` bytes.
fn within(blocks: usize, limit: u64) -> Result<(), CacheError> {
    let length = u64::try_from(blocks)
        .ok()
        .and_then(|b| b.checked_mul(BLOCK as u64))
        .ok_or(CacheError::TooLarge)?;
    if length > limit {
        return Err(CacheError::FileTooLarge { length, limit });
    }
    Ok(())
}

/// Chunk `chunk` of grid `grid`, `raw` blocks, decompressed alone into a buffer of its own.
fn decompress(
    found: &[Found<'_>],
    grid: usize,
    chunk: usize,
    bytes: &[u8],
    raw: usize,
    leaf: bool,
) -> Result<Vec<Block>, CacheError> {
    let mut out = vec![Block::new_zeroed(); raw];
    let mut w = Work { grid, chunk, bytes, room: out.as_mut_bytes(), leaf };
    run(found, vec![&mut w])?;
    Ok(out)
}

/// Checks a frozen buffer of `len` bytes held as `head` and `tail`, its leaf values left
/// out, as a view of type `value_type`: the leaves meeting `extent`.
fn meeting(
    value_type: ValueType,
    head: &[u8],
    tail: &[u8],
    len: usize,
    extent: Bounds,
) -> Result<Vec<(u32, Coord)>, FormatError> {
    struct Walk<'a> {
        head: &'a [u8],
        tail: &'a [u8],
        len: usize,
        extent: Bounds,
    }
    impl ForType for Walk<'_> {
        type Out = Result<Vec<(u32, Coord)>, FormatError>;
        fn run<T: Stored>(self) -> Self::Out {
            let view = GridView::<T>::nodes(self.head, self.tail, self.len)?;
            match self.extent {
                Bounds::Empty => Ok(Vec::new()),
                Bounds::Box { min, max } => view.leaves_meeting(min, max),
            }
        }
    }
    with_type(value_type, Walk { head, tail, len, extent })
}

impl Region {
    /// The grid's metadata.
    pub fn meta(&self) -> &GridMeta {
        &self.meta
    }

    /// The bounds the archive states for the grid, not checked against it, as
    /// `GridEntry::bounds` gives them.
    pub fn bounds(&self) -> Bounds {
        self.bounds
    }

    /// The type of the grid's values.
    pub fn value_type(&self) -> ValueType {
        self.values.0
    }

    /// How the grid's leaf values are encoded, as `GridEntry::encoding` gives it.
    pub fn encoding(&self) -> Encoding {
        self.values.1
    }

    /// The box read.
    pub fn extent(&self) -> Bounds {
        self.extent
    }

    /// A view of the region's values of type `T`; another type is refused as
    /// `CacheError::Grid` with `FormatError::WrongValueType`.
    pub fn view<T: Stored>(&self) -> Result<RegionView<'_, T>, CacheError> {
        let grid = |error| CacheError::Grid { index: self.index, error };
        let nodes = GridView::<T>::nodes(self.head.as_bytes(), self.tail.as_bytes(), self.len)
            .map_err(grid)?;
        let mut chunks = Vec::new();
        for (k, blocks) in &self.chunks {
            let leaves = <[[T; LEAF_VOXELS]]>::ref_from_bytes(blocks.as_bytes())
                .map_err(|_| grid(FormatError::Inconsistent("leaf value")))?;
            chunks.push((*k, leaves));
        }
        Ok(RegionView { nodes, chunks, leaves: &self.leaves, extent: self.extent })
    }
}

/// A region's values read in place (`Region::view`): every voxel within its box answered as
/// the grid's frozen buffer answers it, and every other refused as
/// `FormatError::OutsideRegion`, whatever the chunks held cover (docs/formats.md §8.6).
#[derive(Debug, Clone)]
pub struct RegionView<'a, T> {
    nodes: GridView<'a, T>,
    chunks: Vec<(usize, &'a [[T; LEAF_VOXELS]])>,
    leaves: &'a [(u32, Coord)],
    extent: Bounds,
}

impl<'a, T: Stored> RegionView<'a, T> {
    /// The box read.
    pub fn extent(&self) -> Bounds {
        self.extent
    }

    /// The value outside the tree.
    pub fn background(&self) -> T {
        self.nodes.background()
    }

    /// The number and origin of every leaf whose voxels meet the box, in number order.
    pub fn leaves(&self) -> impl Iterator<Item = (u32, Coord)> + '_ {
        self.leaves.iter().copied()
    }

    /// What holds a voxel within the box.
    pub fn locate(&self, v: Coord) -> Result<ViewLocation, FormatError> {
        let inside = match self.extent {
            Bounds::Empty => false,
            Bounds::Box { min, max } => (0..3).all(|k| {
                v.get(k)
                    .zip(min.get(k))
                    .zip(max.get(k))
                    .is_some_and(|((c, lo), hi)| lo <= c && c <= hi)
            }),
        };
        if !inside {
            return Err(FormatError::OutsideRegion);
        }
        self.nodes.locate(v)
    }

    /// The value at a voxel within the box: its leaf's, its tile's, or the background.
    pub fn get(&self, v: Coord) -> Result<T, FormatError> {
        match self.locate(v)? {
            ViewLocation::Leaf(n) => self
                .values(n)?
                .get(local_index(v))
                .copied()
                .ok_or(FormatError::Inconsistent("leaf value")),
            ViewLocation::Tile(t, _) => self.nodes.tile_value(t),
            ViewLocation::Background => Ok(self.nodes.background()),
        }
    }

    /// A leaf's 512 values, x fastest, for a leaf meeting the box; another is refused as
    /// `FormatError::OutsideRegion`.
    pub fn leaf(&self, n: u32) -> Result<&'a [T; LEAF_VOXELS], FormatError> {
        self.leaves
            .binary_search_by_key(&n, |(m, _)| *m)
            .map_err(|_| FormatError::OutsideRegion)?;
        self.values(n)
    }

    /// A leaf's 512 values from its chunk, for a leaf known to meet the box: one that holds
    /// a voxel within it, as `get` finds.
    fn values(&self, n: u32) -> Result<&'a [T; LEAF_VOXELS], FormatError> {
        let n = n as usize;
        let lost = FormatError::Inconsistent("leaf value");
        let i =
            self.chunks.binary_search_by_key(&(n / CHUNK_LEAVES), |(k, _)| *k).map_err(|_| lost)?;
        self.chunks.get(i).and_then(|(_, l)| l.get(n % CHUNK_LEAVES)).ok_or(lost)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::Field;
    use crate::meta::GridMeta;
    use crate::topology::Topology;

    /// Entries of 1100 leaves, three chunks of them, and a tile, of `f32` and `[f64; 3]`, and of none.
    fn entries() -> Vec<CacheEntry> {
        let empty = Topology::new();
        let mut e = empty.edit();
        for i in 0..1100 {
            e.activate([8 * i, 0, 8 * (i % 3)]).unwrap();
        }
        e.add_tile(crate::tree::TileLevel::Lower, [0, 4096, 0]).unwrap();
        let t = e.commit().unwrap().0;
        let meta = GridMeta::new("g", [1.0; 3], "mm").unwrap();
        vec![
            CacheEntry::new(&meta, &t, &Field::new(&t, 1.5_f32)).unwrap(),
            CacheEntry::new(&meta, &t, &Field::new(&t, [0.5_f64, 1.0, -1.0])).unwrap(),
            CacheEntry::new(&meta, &empty, &Field::new(&empty, 0_u8)).unwrap(),
        ]
    }

    /// An archive is read back as the cache file of the same grids (docs/formats.md §8):
    /// the bytes it decompresses into are the cache file's, byte for byte.
    #[test]
    fn an_archive_reads_back_as_the_cache_file_of_the_same_grids() {
        let e = entries();
        let file = from_bytes(&to_bytes(&e).unwrap(), u64::MAX).unwrap();
        assert!(file.storage.bytes() == crate::cache::to_bytes(&e).unwrap().as_slice());
        let leaves = 16 * 4 * 1100;
        // The frozen buffer itself, aligned as the header needs; a copy may not be.
        fn frozen(g: &CacheEntry) -> &[u8] {
            match &g.payload {
                Payload::Grid { frozen, .. } => frozen.as_bytes(),
                Payload::Bytes { .. } => panic!("a grid"),
            }
        }
        let starts: Vec<Vec<_>> = e.iter().map(|g| cuts(frozen(g)).unwrap()).collect();
        assert_eq!(starts[0].len(), 5, "the nodes, three chunks of leaves and the tiles");
        let flags: Vec<_> = starts[0].iter().map(|(_, leaf)| *leaf).collect();
        assert_eq!(flags, [false, true, true, true, false]);
        assert_eq!(starts[0][1..4].iter().map(|(r, _)| r).sum::<usize>(), leaves);
        assert_eq!(starts[1][1..4].iter().map(|(r, _)| r).sum::<usize>(), leaves * 6);
    }

    /// A grid cut otherwise than docs/formats.md §8.3 cuts it is refused (§8.4, rule 6),
    /// though each chunk decompresses: here into one chunk.
    #[test]
    fn a_grid_cut_otherwise_is_refused() {
        let e = entries();
        let mut bytes = Vec::new();
        emit(&exact(&e), |b| Ok(vec![(b.len() / BLOCK, false)]), &mut bytes).unwrap();
        assert!(matches!(from_bytes(&bytes, u64::MAX), Err(CacheError::Chunks(0))));
    }
}
