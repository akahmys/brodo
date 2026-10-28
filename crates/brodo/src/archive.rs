//! The archive profile (ARCHITECTURE.md §4.7, ADR-0016): grids with their metadata in one
//! file, each frozen buffer cut into chunks compressed apart by `lz4_flex`'s block format,
//! read back as the cache file of the same grids. This module parses untrusted bytes: it is
//! listed in `UNTRUSTED_INPUT`, indexes nothing (CODING.md Rule 3), checks the whole file
//! before it answers, and is fuzzed (TESTING.md T6).
//!
//! Format version 1's bytes are in `docs/formats.md` §8, which this code follows.

use std::io::Read;
use std::path::Path;

use rayon::prelude::*;
use zerocopy::{FromBytes, FromZeros, IntoBytes};

use crate::cache::{
    CacheEntry, CacheFile, Cursor, Part, Storage, encode_meta, head, index, pad_to, put_u32, range,
    u32_of, zero,
};
use crate::error::CacheError;
use crate::frozen::{BLOCK, Block, Header};

/// "BRDA", read as a little-endian `u32`.
const MAGIC: u32 = u32::from_le_bytes(*b"BRDA");
/// The format version this build writes and reads (ADR-0016).
const VERSION: u32 = 1;
/// The LZ4 block format, the only codec (ADR-0016).
const CODEC: u32 = 1;
/// Leaves a chunk (ADR-0016).
const CHUNK_LEAVES: usize = 512;
/// Bytes a chunk table entry.
const ENTRY: usize = 8;

/// Each chunk's length in blocks, as docs/formats.md §8.3 cuts a frozen buffer: at its leaf
/// values' start, every `CHUNK_LEAVES` leaves within them and at their end, leaving out a
/// piece of no bytes. The buffer is one `GridView::new` accepts.
fn cuts(frozen: &[u8]) -> Result<Vec<usize>, CacheError> {
    let bad = || CacheError::Inconsistent("grid");
    let (h, _) = Header::ref_from_prefix(frozen).map_err(|_| bad())?;
    let [_, _, _, leaves, _] = h.counts.map(|c| usize::try_from(c.get()).unwrap_or(usize::MAX));
    let [_, _, _, start, _] = h.offsets.map(|c| usize::try_from(c.get()).unwrap_or(usize::MAX));
    let leaf = usize::try_from(h.value_size.get())
        .map_err(|_| bad())?
        .checked_mul(512 / BLOCK)
        .ok_or_else(bad)?;
    let chunk = leaf.checked_mul(CHUNK_LEAVES).ok_or_else(bad)?;
    let end = leaves.checked_mul(leaf).and_then(|n| n.checked_add(start)).ok_or_else(bad)?;
    let total = frozen.len() / BLOCK;
    if end > total {
        return Err(bad());
    }
    let mut marks = vec![0];
    marks.extend((start..end).step_by(chunk));
    marks.extend([end, total]);
    Ok(marks
        .windows(2)
        .filter_map(|w| match w {
            [a, b] if b > a => Some(b - a),
            _ => None,
        })
        .collect())
}

/// A grid as written: its metadata, its chunks' lengths in blocks, and its chunks
/// compressed.
struct Packed {
    meta: Vec<u8>,
    raw: Vec<usize>,
    chunks: Vec<Vec<u8>>,
}

/// A grid's chunks, its frozen buffer cut by `cut`.
fn pack(e: &CacheEntry, cut: Cut) -> Result<Packed, CacheError> {
    let bytes = e.frozen.as_bytes();
    let raw = cut(bytes)?;
    let mut pieces = Vec::new();
    let mut rest = bytes;
    for r in &raw {
        let (piece, after) =
            rest.split_at_checked(r * BLOCK).ok_or(CacheError::Inconsistent("grid"))?;
        pieces.push(piece);
        rest = after;
    }
    // Compressed in parallel, collected in order: the same bytes whatever the threads.
    let chunks = pieces.par_iter().map(|p| lz4_flex::block::compress(p)).collect();
    Ok(Packed { meta: encode_meta(&e.meta, e.bounds)?, raw, chunks })
}

/// How a frozen buffer is cut into chunks: `cuts`, but for a test of the reader.
type Cut = fn(&[u8]) -> Result<Vec<usize>, CacheError>;

/// Writes the archive holding `entries` to `out`, part by part, each frozen buffer cut by
/// `cut`.
fn emit(entries: &[CacheEntry], cut: Cut, out: &mut impl std::io::Write) -> Result<(), CacheError> {
    let packed = entries.iter().map(|e| pack(e, cut)).collect::<Result<Vec<_>, _>>()?;
    let mut top = Vec::new();
    for x in [MAGIC, VERSION, u32_of(entries.len())?, CODEC, u32_of(CHUNK_LEAVES)?, 0, 0, 0] {
        put_u32(&mut top, x);
    }
    let mut at = 1 + entries.len();
    for (e, p) in entries.iter().zip(&packed) {
        let table_at = at + p.meta.len().div_ceil(BLOCK);
        let first = table_at + (p.chunks.len() * ENTRY).div_ceil(BLOCK);
        at = first + p.chunks.iter().map(|c| c.len().div_ceil(BLOCK)).sum::<usize>();
        let grid_blocks = e.frozen.as_bytes().len() / BLOCK;
        for x in [
            u32_of(table_at - p.meta.len().div_ceil(BLOCK))?,
            u32_of(p.meta.len())?,
            u32_of(table_at)?,
            u32_of(p.chunks.len())?,
            u32_of(grid_blocks)?,
            e.value_type,
            0,
            0,
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
        for part in std::iter::once(&p.meta).chain([&table]).chain(&p.chunks) {
            out.write_all(part)?;
            written += part.len();
            pad_to(out, written.next_multiple_of(BLOCK), &mut written)?;
        }
    }
    Ok(())
}

/// The bytes of an archive holding `entries`, in order.
pub fn to_bytes(entries: &[CacheEntry]) -> Result<Vec<u8>, CacheError> {
    let mut out = Vec::new();
    emit(entries, cuts, &mut out)?;
    Ok(out)
}

/// Writes an archive holding `entries`, replacing `path` whole as `cache::write` does.
pub fn write(path: &Path, entries: &[CacheEntry]) -> Result<(), CacheError> {
    crate::cache::replace(path, |out| emit(entries, cuts, out))
}

/// A grid as read: where its metadata is, its frozen buffer's length in blocks, its value
/// type, and its chunks with their lengths in blocks.
struct Found<'a> {
    meta: &'a [u8],
    grid_blocks: usize,
    value_type: u32,
    chunks: Vec<(&'a [u8], usize)>,
}

/// Reads the header block: the magic number, this build's format version, codec and
/// chunk size, and unused words of zero. Returns the grid count.
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
/// and nothing after the last: every grid's metadata, chunk table and chunks.
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
    let [meta_at, meta_len, table_at, chunk_count, grid_blocks, value_type, r0, r1] =
        [(); 8].map(|()| c.u32());
    if [r0?, r1?] != [0; 2] {
        return Err(CacheError::Inconsistent("directory"));
    }
    let meta = range(meta_at?, size(meta_len?)?, after, all.len(), "metadata")?;
    let table_len = size(chunk_count?)?.checked_mul(ENTRY).ok_or(CacheError::TooLarge)?;
    let table_start = meta.end.next_multiple_of(BLOCK);
    let table = range(table_at?, table_len, table_start, all.len(), "chunk table")?;
    let mut parts = vec![meta.clone(), table.clone()];
    let chunks = chunks(all, table, &mut parts)?;
    let grid_blocks = size(grid_blocks?)?;
    let sum = chunks.iter().try_fold(0_usize, |s, (_, raw)| s.checked_add(*raw));
    if sum != Some(grid_blocks) {
        return Err(CacheError::Inconsistent("chunk table"));
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
    let found = Found { meta, grid_blocks, value_type: value_type?, chunks };
    Ok((found, end))
}

/// The chunks a grid's table at `table` lists, each with its length in blocks, none zero,
/// each at the block after the part before it; their places are added to `parts`.
fn chunks<'a>(
    all: &'a [u8],
    table: std::ops::Range<usize>,
    parts: &mut Vec<std::ops::Range<usize>>,
) -> Result<Vec<(&'a [u8], usize)>, CacheError> {
    let what = "chunk table";
    let mut t = Cursor { rest: all.get(table).ok_or(CacheError::Truncated(what))?, what };
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
    let grids = index(out.as_bytes())?;
    for (index, (g, e)) in found.iter().zip(&grids).enumerate() {
        let frozen = out.as_bytes().get(e.grid.clone()).ok_or(CacheError::Truncated("grid"))?;
        if cuts(frozen)?.into_iter().ne(g.chunks.iter().map(|(_, raw)| *raw)) {
            return Err(CacheError::Chunks(index));
        }
    }
    Ok(CacheFile { storage: Storage::Copied(out), grids })
}

/// The cache file of `blocks` blocks that `found` decompresses into, below its header and
/// directory `top`: each grid's metadata, then its chunks decompressed in parallel.
fn unpack(found: &[Found<'_>], top: &[u8], blocks: usize) -> Result<Vec<Block>, CacheError> {
    let mut out = vec![Block::new_zeroed(); blocks];
    let mut rest = out.as_mut_bytes();
    let (lead, after) = rest.split_at_mut_checked(top.len()).ok_or(CacheError::TooLarge)?;
    lead.copy_from_slice(top);
    rest = after;
    // Each grid's metadata, then the room its chunks decompress into, in order.
    let mut work = Vec::new();
    for (i, g) in found.iter().enumerate() {
        let (meta, after) = rest.split_at_mut_checked(g.meta.len()).ok_or(CacheError::TooLarge)?;
        meta.copy_from_slice(g.meta);
        let pad = g.meta.len().next_multiple_of(BLOCK) - g.meta.len();
        let (_, mut after) = after.split_at_mut_checked(pad).ok_or(CacheError::TooLarge)?;
        for (k, (chunk, raw)) in g.chunks.iter().enumerate() {
            let (room, next) =
                after.split_at_mut_checked(raw * BLOCK).ok_or(CacheError::TooLarge)?;
            work.push(((i, k), *chunk, room));
            after = next;
        }
        rest = after;
    }
    // Decompressed in parallel; the first chunk in order that fails is named, whatever the
    // threads.
    let failed: Vec<_> = work
        .par_iter_mut()
        .map(|((i, k), chunk, room)| {
            let filled = lz4_flex::block::decompress_into(chunk, room).ok();
            (filled != Some(room.len())).then_some((*i, *k))
        })
        .collect();
    if let Some((index, chunk)) = failed.into_iter().flatten().next() {
        return Err(CacheError::Decompress { index, chunk });
    }
    Ok(out)
}

/// Reads and checks an archive of at most `limit` bytes, decompressed into a cache file of
/// at most `limit` bytes: a longer file is refused by its length before any of it is read,
/// or by the bytes read where its length says less than it holds, and a larger cache file
/// by the lengths the archive states, before it is made (ADR-0036).
pub fn read(path: &Path, limit: u64) -> Result<CacheFile, CacheError> {
    let file = std::fs::File::open(path)?;
    let length = file.metadata()?.len();
    if length > limit {
        return Err(CacheError::FileTooLarge { length, limit });
    }
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1)).read_to_end(&mut bytes)?;
    let read = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if read > limit {
        return Err(CacheError::FileTooLarge { length: read, limit });
    }
    from_bytes(&bytes, limit)
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
        let starts: Vec<_> = e.iter().map(|g| cuts(g.frozen.as_bytes()).unwrap()).collect();
        assert_eq!(starts[0].len(), 5, "the nodes, three chunks of leaves and the tiles");
        assert_eq!(starts[0][1..4].iter().sum::<usize>(), leaves);
        assert_eq!(starts[1][1..4].iter().sum::<usize>(), leaves * 6);
    }

    /// A grid cut otherwise than docs/formats.md §8.3 cuts it is refused (§8.4, rule 6),
    /// though each chunk decompresses: here into one chunk.
    #[test]
    fn a_grid_cut_otherwise_is_refused() {
        let e = entries();
        let mut bytes = Vec::new();
        emit(&e, |b| Ok(vec![b.len() / BLOCK]), &mut bytes).unwrap();
        assert!(matches!(from_bytes(&bytes, u64::MAX), Err(CacheError::Chunks(0))));
    }
}
