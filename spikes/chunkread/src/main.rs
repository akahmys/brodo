//! Spike for B-027: an archive's region, listing and grid by name read from only the bytes
//! they need, against the file read whole as brodo reads it today (ADR-0016). The seeking
//! reader here is written from docs/formats.md §8 alone; it finds the chunks a box needs
//! from brodo's own region read, done once outside the time, and checks what it
//! decompresses against brodo's view.
//!
//! `chunkread-spike [EDGE] [GRIDS] [ROUNDS]`: writes `out/spike/chunkread/big.archive`, GRIDS
//! dense grids of `f32` of EDGE voxels a side, through a descriptor that keeps nothing in
//! the page cache, then times each case's variants in turn, every read but the warm ones
//! through such a descriptor, and prints one row per variant.

use std::collections::BTreeSet;
use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use brodo::archive;
use brodo::cache::CacheEntry;
use brodo::field::Field;
use brodo::meta::{Bounds, GridMeta};
use brodo::parallel::par_update;
use brodo::topology::Topology;
use rayon::prelude::*;

type R<T> = Result<T, Box<dyn Error + Send + Sync>>;

const BLOCK: u64 = 32;
const LEAF_BYTES: usize = 512 * 4;
const F_NOCACHE: i32 = 48;

unsafe extern "C" {
    fn fcntl(fd: i32, cmd: i32, ...) -> i32;
    fn getloadavg(loads: *mut f64, n: i32) -> i32;
    fn mmap(addr: *mut u8, len: usize, prot: i32, flags: i32, fd: i32, off: i64) -> *mut u8;
    fn msync(addr: *mut u8, len: usize, flags: i32) -> i32;
    fn munmap(addr: *mut u8, len: usize) -> i32;
}

const PROT_READ: i32 = 1;
const MAP_SHARED: i32 = 1;
const MS_INVALIDATE: i32 = 2;

/// Drops `path`'s pages from the page cache: measured on this machine to bring a 2 GiB
/// file's read from 16 GB/s, the cache's, to 6.5 GB/s, the disk's.
fn evict(path: &Path) -> R<()> {
    let f = File::open(path)?;
    let len = usize::try_from(f.metadata()?.len())?;
    // SAFETY: a read-only shared mapping of a file this process opened, unmapped before
    // return; nothing reads through it.
    unsafe {
        let m = mmap(std::ptr::null_mut(), len, PROT_READ, MAP_SHARED, f.as_raw_fd(), 0);
        if m as isize == -1 {
            return Err("mmap failed".into());
        }
        let r = msync(m, len, MS_INVALIDATE);
        munmap(m, len);
        if r != 0 {
            return Err("msync failed".into());
        }
    }
    Ok(())
}

fn loads() -> [f64; 3] {
    let mut l = [0.0; 3];
    // SAFETY: getloadavg writes at most three doubles into `l`.
    unsafe { getloadavg(l.as_mut_ptr(), 3) };
    l
}

/// Opens `path`, its reads kept out of the page cache unless `cached`.
fn open(path: &Path, cached: bool) -> R<File> {
    let f = File::open(path)?;
    if !cached {
        // SAFETY: fcntl on a descriptor this process owns.
        if unsafe { fcntl(f.as_raw_fd(), F_NOCACHE, 1) } != 0 {
            return Err("F_NOCACHE refused".into());
        }
    }
    Ok(f)
}

/// A file, with the bytes and calls read from it counted.
struct Reader {
    file: File,
    bytes: AtomicU64,
    calls: AtomicU64,
}

impl Reader {
    fn new(path: &Path, cached: bool) -> R<Self> {
        Ok(Self { file: open(path, cached)?, bytes: AtomicU64::new(0), calls: AtomicU64::new(0) })
    }
    fn at(&self, at: u64, len: u64) -> R<Vec<u8>> {
        let mut buf = vec![0_u8; usize::try_from(len)?];
        self.file.read_exact_at(&mut buf, at)?;
        self.bytes.fetch_add(len, Ordering::Relaxed);
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(buf)
    }
    /// The file whole, read as `std::fs::read` reads it: calls of at most 2 GiB − 1, the
    /// most macOS takes in one.
    fn whole(&self) -> R<Vec<u8>> {
        let len = self.file.metadata()?.len();
        let mut buf = vec![0_u8; usize::try_from(len)?];
        for (k, piece) in buf.chunks_mut(i32::MAX as usize).enumerate() {
            self.file.read_exact_at(piece, k as u64 * i32::MAX as u64)?;
            self.calls.fetch_add(1, Ordering::Relaxed);
        }
        self.bytes.fetch_add(len, Ordering::Relaxed);
        Ok(buf)
    }
    fn counts(&self) -> (u64, u64) {
        (self.bytes.load(Ordering::Relaxed), self.calls.load(Ordering::Relaxed))
    }
}

fn u32_at(b: &[u8], at: usize) -> R<u64> {
    let w: [u8; 4] = b.get(at..at + 4).ok_or("short")?.try_into()?;
    Ok(u64::from(u32::from_le_bytes(w)))
}

fn up(n: u64) -> u64 {
    n.next_multiple_of(BLOCK)
}

/// A directory entry (docs/formats.md §8.2).
#[derive(Clone, Copy)]
struct Entry {
    meta_at: u64,
    meta_len: u64,
    table_at: u64,
    chunk_count: u64,
}

impl Entry {
    fn table_len(&self) -> u64 {
        self.chunk_count * 8
    }
    /// From the metadata's first byte to the chunk table's last.
    fn span(&self) -> (u64, u64) {
        let start = self.meta_at * BLOCK;
        (start, self.table_at * BLOCK + self.table_len() - start)
    }
}

/// The header and directory: two reads.
fn directory(r: &Reader) -> R<Vec<Entry>> {
    let head = r.at(0, BLOCK)?;
    if head.get(..4) != Some(b"BRDA") {
        return Err("not an archive".into());
    }
    let count = u32_at(&head, 8)?;
    let dir = r.at(BLOCK, count * BLOCK)?;
    (0..usize::try_from(count)?)
        .map(|i| {
            let b = i * 32;
            if u32_at(&dir, b + 24)? != 0 {
                return Err("the spike reads exact grids only".into());
            }
            Ok(Entry {
                meta_at: u32_at(&dir, b)?,
                meta_len: u32_at(&dir, b + 4)?,
                table_at: u32_at(&dir, b + 8)?,
                chunk_count: u32_at(&dir, b + 12)?,
            })
        })
        .collect()
}

/// Each chunk's place, compressed length and length before compression, from a table.
fn places(e: &Entry, span: &[u8]) -> R<Vec<(u64, u64, u64)>> {
    let off = usize::try_from(e.table_at * BLOCK - e.meta_at * BLOCK)?;
    let mut at = up(e.table_at * BLOCK + e.table_len());
    let mut out = Vec::new();
    for k in 0..usize::try_from(e.chunk_count)? {
        let raw = u32_at(span, off + k * 8)? * BLOCK;
        let len = u32_at(span, off + k * 8 + 4)?;
        out.push((at, len, raw));
        at = up(at + len);
    }
    Ok(out)
}

fn decompress(bytes: &[u8], raw: u64) -> R<Vec<u8>> {
    let mut out = vec![0_u8; usize::try_from(raw)?];
    let n = lz4_flex::block::decompress_into(bytes, &mut out)?;
    if n != out.len() {
        return Err("short chunk".into());
    }
    Ok(out)
}

#[derive(Clone, Copy, PartialEq)]
enum Reads {
    /// Runs of neighbouring chunks read in one call each.
    Merged,
    /// One call a chunk.
    Each,
    /// One call a chunk, the calls in parallel.
    Parallel,
}

/// Chunks `wanted` of grid entry `place`, read as `how` says and decompressed in parallel;
/// with `all`, every entry's metadata and table is read too.
fn seek(
    r: &Reader,
    place: usize,
    wanted: &BTreeSet<usize>,
    how: Reads,
    all: bool,
) -> R<Vec<(usize, Vec<u8>)>> {
    let dir = directory(r)?;
    if all {
        for (i, e) in dir.iter().enumerate() {
            if i != place {
                let (at, len) = e.span();
                r.at(at, len)?;
            }
        }
    }
    let e = dir.get(place).ok_or("no entry")?;
    let (at, len) = e.span();
    let span = r.at(at, len)?;
    let chunks = places(e, &span)?;
    let get = |k: usize| chunks.get(k).copied().ok_or("no chunk");
    let (c0, l0, raw0) = get(0)?;
    // The first chunk is decompressed before any other: its nodes say which leaf chunks
    // the box needs, a walk this spike takes from brodo's region read instead.
    let first = decompress(&r.at(c0, l0)?, raw0)?;
    let mut out = vec![(0, first)];
    let rest: Vec<usize> = wanted.iter().copied().filter(|k| *k != 0).collect();
    match how {
        Reads::Parallel => {
            let got: R<Vec<_>> = rest
                .par_iter()
                .map(|k| {
                    let (at, len, raw) = get(*k)?;
                    Ok((*k, decompress(&r.at(at, len)?, raw)?))
                })
                .collect();
            out.extend(got?);
        }
        Reads::Each | Reads::Merged => {
            let mut runs: Vec<Vec<usize>> = Vec::new();
            for k in rest {
                match runs.last_mut() {
                    Some(run) if how == Reads::Merged && run.last() == Some(&(k - 1)) => run.push(k),
                    _ => runs.push(vec![k]),
                }
            }
            let mut read = Vec::new();
            for run in runs {
                let (start, _, _) = get(run[0])?;
                let (last, len, _) = get(*run.last().ok_or("empty run")?)?;
                read.push((run, start, r.at(start, last + len - start)?));
            }
            let pieces: Vec<(usize, u64, &[u8])> = read
                .iter()
                .flat_map(|(run, start, bytes)| run.iter().map(move |k| (*k, *start, bytes.as_slice())))
                .collect();
            let got: R<Vec<_>> = pieces
                .par_iter()
                .map(|(k, start, bytes)| {
                    let (at, len, raw) = get(*k)?;
                    let from = usize::try_from(at - start)?;
                    let piece = bytes.get(from..from + usize::try_from(len)?).ok_or("short run")?;
                    Ok((*k, decompress(piece, raw)?))
                })
                .collect();
            out.extend(got?);
        }
    }
    Ok(out)
}

/// The listing's reads: the header and directory, and each entry's metadata, with its
/// table if `tables`.
fn seek_list(r: &Reader, tables: bool) -> R<u64> {
    let dir = directory(r)?;
    let mut n = 0;
    for e in &dir {
        let (at, len) = if tables { e.span() } else { (e.meta_at * BLOCK, e.meta_len) };
        n += r.at(at, len)?.len() as u64;
    }
    Ok(n)
}

/// A dense grid of `edge` voxels a side, every leaf present, its values a smooth field
/// that differs by `k`.
fn grid(k: usize, edge: i32) -> R<CacheEntry> {
    let empty = Topology::new();
    let mut e = empty.edit();
    for z in (0..edge).step_by(8) {
        for y in (0..edge).step_by(8) {
            for x in (0..edge).step_by(8) {
                e.activate([x, y, z])?;
            }
        }
    }
    let t = e.commit()?.0;
    let mut f = Field::new(&t, 0.0_f32);
    let s = 0.02 * (k as f32 + 1.0);
    par_update(&t, &mut f, |n, values| -> R<()> {
        let voxels = t.leaf_voxels(n)?;
        for (v, out) in voxels.zip(values.iter_mut()) {
            let [x, y, z] = v.map(|c| c as f32 * s);
            *out = x.sin() * y.cos() + 0.5 * z.sin();
        }
        Ok(())
    })?;
    Ok(CacheEntry::new(&GridMeta::new(&format!("g{k}"), [1.0; 3], "voxel")?, &t, &f)?)
}

/// Writes the archive through a descriptor that keeps nothing in the page cache.
fn write(path: &Path, edge: i32, grids: usize) -> R<u64> {
    let entries: R<Vec<_>> = (0..grids).map(|k| grid(k, edge)).collect();
    let bytes = archive::to_bytes(&entries?)?;
    let _ = std::fs::remove_file(path);
    let mut f = File::create(path)?;
    // SAFETY: fcntl on a descriptor this process owns.
    if unsafe { fcntl(f.as_raw_fd(), F_NOCACHE, 1) } != 0 {
        return Err("F_NOCACHE refused".into());
    }
    f.write_all(&bytes)?;
    f.sync_all()?;
    Ok(bytes.len() as u64)
}

type Run<'a> = Box<dyn Fn(&Reader) -> R<u64> + 'a>;

/// Times each variant `rounds` times in turn, each on a fresh descriptor, and prints its
/// bytes, calls, fastest and median.
fn time(case: &str, path: &Path, rounds: usize, variants: &[(&str, bool, Run<'_>)]) -> R<()> {
    let l = loads();
    let mut times = vec![Vec::new(); variants.len()];
    let mut counts = vec![(0, 0, 0); variants.len()];
    for (_, cached, run) in variants.iter().filter(|v| v.1) {
        run(&Reader::new(path, *cached)?)?;
    }
    for _ in 0..rounds {
        for (i, (_, cached, run)) in variants.iter().enumerate() {
            if !*cached {
                evict(path)?;
            }
            let r = Reader::new(path, *cached)?;
            let t = Instant::now();
            let answer = run(&r)?;
            times[i].push(t.elapsed().as_secs_f64() * 1e3);
            let (b, c) = r.counts();
            counts[i] = (b, c, answer);
        }
    }
    for (i, (name, _, _)) in variants.iter().enumerate() {
        let t = &mut times[i];
        t.sort_by(f64::total_cmp);
        let (b, c, answer) = counts[i];
        println!(
            "{case}\t{name}\t{b}\t{c}\t{answer}\t{:.3}\t{:.3}\t{:.2} {:.2}",
            t[0],
            t[t.len() / 2],
            l[0],
            l[1]
        );
    }
    Ok(())
}

fn main() -> R<()> {
    let args: Vec<String> = std::env::args().collect();
    let edge: i32 = args.get(1).map_or(Ok(512), |a| a.parse())?;
    let grids: usize = args.get(2).map_or(Ok(4), |a| a.parse())?;
    let rounds: usize = args.get(3).map_or(Ok(7), |a| a.parse())?;
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../out/spike/chunkread");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("big.archive");
    let len = write(&path, edge, grids)?;
    eprintln!("archive {len} bytes, {grids} grids of {edge}³, rayon {}", rayon::current_num_threads());
    println!("case\tvariant\tbytes\tcalls\tanswer\tfastest_ms\tmedian_ms\tload");

    let all = std::fs::read(&path)?; // outside the time, for brodo's answers to check against
    let mid = edge / 2;
    let boxes = [
        ("region_leaf", Bounds::Box { min: [mid; 3], max: [mid; 3] }),
        ("region_0.1", Bounds::Box { min: [mid - edge / 20; 3], max: [mid + edge / 20; 3] }),
        ("region_0.5", Bounds::Box { min: [edge / 4; 3], max: [edge * 3 / 4 - 1; 3] }),
    ];
    let place = 1.min(grids - 1);
    for (case, extent) in boxes {
        let region = archive::region_from_bytes(&all, place, extent, u64::MAX)?;
        let view = region.view::<f32>()?;
        let leaves: Vec<(u32, _)> = view.leaves().collect();
        let wanted: BTreeSet<usize> = leaves.iter().map(|(n, _)| *n as usize / 512 + 1).collect();
        // Checks the seeking reader's chunks against brodo's view, outside the time.
        let got = seek(&Reader::new(&path, true)?, place, &wanted, Reads::Merged, false)?;
        for (n, _) in &leaves {
            let k = *n as usize / 512 + 1;
            let chunk = &got.iter().find(|(j, _)| *j == k).ok_or("chunk missing")?.1;
            let from = (*n as usize % 512) * LEAF_BYTES;
            let ours = chunk.get(from..from + LEAF_BYTES).ok_or("leaf beyond chunk")?;
            let theirs: Vec<u8> = view.leaf(*n)?.iter().flat_map(|v| v.to_le_bytes()).collect();
            if ours != theirs.as_slice() {
                return Err(format!("{case}: leaf {n} differs").into());
            }
        }
        eprintln!("{case}: {} leaves, {} leaf chunks, checked", leaves.len(), wanted.len());
        let whole: Run<'_> = Box::new(|r| {
            let region = archive::region_from_bytes(&r.whole()?, place, extent, u64::MAX)?;
            Ok(region.view::<f32>()?.leaves().count() as u64)
        });
        let seeking = |how, all| -> Run<'_> {
            let wanted = &wanted;
            Box::new(move |r| Ok(seek(r, place, wanted, how, all)?.len() as u64))
        };
        let warm_whole: Run<'_> = Box::new(|r| {
            let region = archive::region_from_bytes(&r.whole()?, place, extent, u64::MAX)?;
            Ok(region.view::<f32>()?.leaves().count() as u64)
        });
        time(
            case,
            &path,
            rounds,
            &[
                ("whole", false, whole),
                ("seek_merged", false, seeking(Reads::Merged, false)),
                ("seek_each", false, seeking(Reads::Each, false)),
                ("seek_parallel", false, seeking(Reads::Parallel, false)),
                ("seek_merged_all_tables", false, seeking(Reads::Merged, true)),
                ("whole_warm", true, warm_whole),
                ("seek_merged_warm", true, seeking(Reads::Merged, false)),
            ],
        )?;
    }

    let listed = archive::list_from_bytes(&all)?;
    let named = listed.grids_named(&format!("g{place}"));
    let g = named.first().ok_or("no grid of the name")?;
    let (index, bounds) = (g.index(), g.bounds());
    let count = archive::region_from_bytes(&all, index, bounds, u64::MAX)?.view::<f32>()?.leaves().count();
    eprintln!("listing: {} grids; by name: grid {index}, {count} leaves", listed.grids().len());
    let chunks: BTreeSet<usize> = (0..=count.div_ceil(512)).collect();
    time(
        "listing",
        &path,
        rounds,
        &[
            ("whole", false, Box::new(|r| Ok(archive::list_from_bytes(&r.whole()?)?.grids().len() as u64))),
            ("seek_meta", false, Box::new(|r| seek_list(r, false))),
            ("seek_meta_tables", false, Box::new(|r| seek_list(r, true))),
        ],
    )?;
    let name = format!("g{place}");
    time(
        "by_name",
        &path,
        rounds,
        &[
            (
                "whole",
                false,
                Box::new(|r| {
                    let bytes = r.whole()?;
                    let listing = archive::list_from_bytes(&bytes)?;
                    let g = listing.grids_named(&name).into_iter().next().ok_or("no grid")?;
                    let region = archive::region_from_bytes(&bytes, g.index(), g.bounds(), u64::MAX)?;
                    Ok(region.view::<f32>()?.leaves().count() as u64)
                }),
            ),
            (
                "seek_merged",
                false,
                Box::new(|r| {
                    seek_list(r, false)?;
                    Ok(seek(r, index, &chunks, Reads::Merged, false)?.len() as u64)
                }),
            ),
            (
                "seek_parallel",
                false,
                Box::new(|r| {
                    seek_list(r, false)?;
                    Ok(seek(r, index, &chunks, Reads::Parallel, false)?.len() as u64)
                }),
            ),
        ],
    )?;
    Ok(())
}
