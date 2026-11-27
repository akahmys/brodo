//! Spike for B-032: an archive read whole, as brodo reads it today, against one read entry
//! by entry, each entry's chunks taken and decompressed before the next is read
//! (issue 24). The two readers here are written from docs/formats.md §3 and §8 alone; each
//! gives the cache file's bytes, held to `cache::to_bytes` of the same entries before
//! anything is timed. Neither checks the cache file as §3.4 asks, the same work for both;
//! `brodo`, `archive::read`, gives the cost of a read with its checks.
//!
//! `entryread-spike write`: writes every case's archive under `out/spike/entryread/`.
//! `entryread-spike time ROUNDS`: times each case's variants in turn, one row a variant.
//! `entryread-spike read CASE VARIANT`: one read, for `/usr/bin/time -l`'s peak memory;
//! VARIANT `none` reads nothing, for the process's own.

use std::error::Error;
use std::fs::File;
use std::hint::black_box;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Instant;

use brodo::archive;
use brodo::cache::{self, CacheEntry};
use brodo::field::Field;
use brodo::meta::GridMeta;
use brodo::parallel::par_update;
use brodo::topology::Topology;
use rayon::prelude::*;

type R<T> = Result<T, Box<dyn Error + Send + Sync>>;

const BLOCK: usize = 32;
/// Bytes written over before each run, as brodo's benchmark harness does (ADR-0019).
const EVICT_BYTES: usize = 256 << 20;

/// A case: its name, and its grids, each a box of leaves (in leaves a side) of `f32`.
struct Case {
    name: &'static str,
    grids: usize,
    leaves: [i32; 3],
}

const CASES: [Case; 5] = [
    // One grid of 32 768 leaves, 64 leaf chunks: a large field alone.
    Case { name: "one_large", grids: 1, leaves: [32, 32, 32] },
    // Four such grids: what the per-entry read saves memory on.
    Case { name: "four_large", grids: 4, leaves: [32, 32, 32] },
    // One grid of 2 048 leaves, 4 leaf chunks: fewer chunks than threads.
    Case { name: "one_medium", grids: 1, leaves: [16, 16, 8] },
    // 64 grids of 1 024 leaves, 2 leaf chunks each: the per-entry read's worst case.
    Case { name: "many_medium", grids: 64, leaves: [16, 8, 8] },
    // 256 grids of 64 leaves, one leaf chunk each.
    Case { name: "many_small", grids: 256, leaves: [4, 4, 4] },
];

fn out_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/spike/entryread")
}

fn path_of(case: &str) -> PathBuf {
    out_dir().join(format!("{case}.archive"))
}

/// Grid `g` of `case`: every leaf of its box present, a smooth field differing by grid.
fn grid(case: &Case, g: usize) -> R<CacheEntry> {
    let empty = Topology::new();
    let mut e = empty.edit();
    let [lx, ly, lz] = case.leaves;
    for z in 0..lz {
        for y in 0..ly {
            for x in 0..lx {
                e.activate([8 * x, 8 * y, 8 * z])?;
            }
        }
    }
    let t = e.commit()?.0;
    let mut f = Field::new(&t, 0.0_f32);
    let k = g as f32 * 0.37;
    par_update(&t, &mut f, |n, values| -> R<()> {
        for (v, o) in t.leaf_voxels(n)?.zip(values.iter_mut()) {
            let [x, y, z] = v.map(|c: i32| c as f32);
            *o = (x * 0.05 + k).sin() * (y * 0.031).cos() + z * 0.002;
        }
        Ok(())
    })?;
    Ok(CacheEntry::new(&GridMeta::new(&format!("g{g}"), [1.0; 3], "voxel")?, &t, &f)?)
}

fn entries(case: &Case) -> R<Vec<CacheEntry>> {
    (0..case.grids).map(|g| grid(case, g)).collect()
}

fn u32_at(b: &[u8], at: usize) -> R<usize> {
    let w: [u8; 4] = b.get(at..at + 4).ok_or("short")?.try_into()?;
    Ok(usize::try_from(u32::from_le_bytes(w))?)
}

/// A directory entry (docs/formats.md §8.2), the words the readers use.
#[derive(Clone, Copy)]
struct Entry {
    meta_at: usize,
    meta_len: usize,
    table_at: usize,
    chunk_count: usize,
    payload_blocks: usize,
    value_type: usize,
    kind: usize,
}

/// The header's count and the directory's entries, from the bytes from the file's start
/// through its directory.
fn directory(lead: &[u8]) -> R<Vec<Entry>> {
    if lead.get(0..4) != Some(b"BRDA") || u32_at(lead, 4)? != 2 {
        return Err("not an archive of version 2".into());
    }
    let count = u32_at(lead, 8)?;
    (0..count)
        .map(|i| {
            let at = (i + 1) * BLOCK;
            let w = |k: usize| u32_at(lead, at + 4 * k);
            if w(6)? != 0 {
                return Err("an encoding other than exact".into());
            }
            Ok(Entry {
                meta_at: w(0)? * BLOCK,
                meta_len: w(1)?,
                table_at: w(2)? * BLOCK,
                chunk_count: w(3)?,
                payload_blocks: w(4)?,
                value_type: w(5)?,
                kind: w(7)?,
            })
        })
        .collect()
}

/// The cache file's length and its header and directory (docs/formats.md §3), and where
/// each entry's metadata goes in it.
fn cache_head(es: &[Entry]) -> R<(Vec<u8>, Vec<usize>)> {
    let mut head = vec![0_u8; (es.len() + 1) * BLOCK];
    head[0..4].copy_from_slice(b"BRDC");
    head[4..8].copy_from_slice(&2_u32.to_le_bytes());
    head[8..12].copy_from_slice(&u32::try_from(es.len())?.to_le_bytes());
    let mut at = head.len();
    let mut places = Vec::new();
    for (i, e) in es.iter().enumerate() {
        let payload_at = at + e.meta_len.next_multiple_of(BLOCK);
        let words = [
            at / BLOCK,
            e.meta_len,
            payload_at / BLOCK,
            e.payload_blocks,
            e.value_type,
            e.kind,
        ];
        for (k, w) in words.into_iter().enumerate() {
            let o = (i + 1) * BLOCK + 4 * k;
            head[o..o + 4].copy_from_slice(&u32::try_from(w)?.to_le_bytes());
        }
        places.push(at);
        at = payload_at + e.payload_blocks * BLOCK;
    }
    places.push(at);
    Ok((head, places))
}

/// Entry `e`'s chunk table, from `bytes` at the table's place less `base`: each chunk's
/// raw length in bytes and compressed length.
fn table(e: &Entry, bytes: &[u8], base: usize) -> R<Vec<(usize, usize)>> {
    (0..e.chunk_count)
        .map(|k| {
            let at = e.table_at - base + 8 * k;
            Ok((u32_at(bytes, at)? * BLOCK, u32_at(bytes, at + 4)?))
        })
        .collect()
}

/// Splits `room`, an entry's payload, into each chunk's, paired with its compressed bytes
/// from `src`, the chunks laid one after another from its start, each padded to a block.
fn pair<'a, 'b>(
    room: &'b mut [u8],
    src: &'a [u8],
    lens: &[(usize, usize)],
    work: &mut Vec<(&'a [u8], &'b mut [u8])>,
) -> R<()> {
    let (mut room, mut at) = (room, 0);
    for (raw, len) in lens {
        let (mine, rest) = room.split_at_mut(*raw);
        work.push((src.get(at..at + len).ok_or("chunk beyond the file")?, mine));
        room = rest;
        at += len.next_multiple_of(BLOCK);
    }
    Ok(())
}

/// Decompresses each chunk into its room, in parallel.
fn decompress(work: &mut [(&[u8], &mut [u8])]) -> R<()> {
    let ok = work.par_iter_mut().all(|(src, room)| {
        lz4_flex::block::decompress_into(src, room).ok() == Some(room.len())
    });
    if ok { Ok(()) } else { Err("a chunk did not decompress to its length".into()) }
}

/// The file read whole, then every entry's chunks decompressed together, in parallel, as
/// brodo reads an archive today.
fn read_whole(path: &Path) -> R<Vec<u8>> {
    let bytes = std::fs::read(path)?;
    let es = directory(&bytes)?;
    let (head, places) = cache_head(&es)?;
    let mut out = vec![0_u8; *places.last().ok_or("no end")?];
    let (lead, mut rest) = out.split_at_mut(head.len());
    lead.copy_from_slice(&head);
    let mut work = Vec::new();
    for e in &es {
        let (meta, after) = rest.split_at_mut(e.meta_len.next_multiple_of(BLOCK));
        meta[..e.meta_len].copy_from_slice(&bytes[e.meta_at..e.meta_at + e.meta_len]);
        let (room, after) = after.split_at_mut(e.payload_blocks * BLOCK);
        let lens = table(e, &bytes, 0)?;
        let first = (e.table_at + 8 * e.chunk_count).next_multiple_of(BLOCK);
        pair(room, &bytes[first..], &lens, &mut work)?;
        rest = after;
    }
    decompress(&mut work)?;
    Ok(out)
}

/// The file read from its start to its end, without a seek, so that a pipe reads too: the
/// header and directory, then each entry's metadata and table, then its chunks, each
/// entry's chunks decompressed, in parallel, before the next entry is read.
fn read_by_entry(path: &Path) -> R<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut lead = vec![0_u8; BLOCK];
    file.read_exact(&mut lead)?;
    let count = u32_at(&lead, 8)?;
    lead.resize((count + 1) * BLOCK, 0);
    file.read_exact(&mut lead[BLOCK..])?;
    let es = directory(&lead)?;
    let (head, places) = cache_head(&es)?;
    let mut out = vec![0_u8; *places.last().ok_or("no end")?];
    let (top, mut rest) = out.split_at_mut(head.len());
    top.copy_from_slice(&head);
    let mut at = lead.len();
    for e in &es {
        // Metadata and table, contiguous, up to the first chunk.
        let first = (e.table_at + 8 * e.chunk_count).next_multiple_of(BLOCK);
        let mut mt = vec![0_u8; first - at];
        file.read_exact(&mut mt)?;
        let (meta, after) = rest.split_at_mut(e.meta_len.next_multiple_of(BLOCK));
        meta[..e.meta_len].copy_from_slice(&mt[e.meta_at - at..e.meta_at - at + e.meta_len]);
        let lens = table(e, &mt, at)?;
        let span: usize = lens.iter().map(|(_, len)| len.next_multiple_of(BLOCK)).sum();
        // Read as `std::fs::read` reads, into room not zeroed first.
        let mut chunks = Vec::with_capacity(span);
        (&mut file).take(u64::try_from(span)?).read_to_end(&mut chunks)?;
        if chunks.len() != span {
            return Err("the file ends inside an entry's chunks".into());
        }
        let (room, after) = after.split_at_mut(e.payload_blocks * BLOCK);
        let mut work = Vec::new();
        pair(room, &chunks, &lens, &mut work)?;
        decompress(&mut work)?;
        rest = after;
        at = first + span;
    }
    let mut tail = [0_u8; 1];
    if file.read(&mut tail)? != 0 {
        return Err("bytes after the last entry".into());
    }
    Ok(out)
}

fn read(variant: &str, path: &Path) -> R<usize> {
    Ok(match variant {
        "whole" => read_whole(path)?.len(),
        "entry" => read_by_entry(path)?.len(),
        "brodo" => archive::read(path, u64::MAX)?.grids().len(),
        "none" => 0,
        v => return Err(format!("no variant {v}").into()),
    })
}

fn write_cases() -> R<()> {
    std::fs::create_dir_all(out_dir())?;
    for case in &CASES {
        let es = entries(case)?;
        let path = path_of(case.name);
        archive::write(&path, &es)?;
        // A read that gives other bytes than the cache file of the same entries is not timed.
        let expected = cache::to_bytes(&es)?;
        for v in ["whole", "entry"] {
            let got = if v == "whole" { read_whole(&path)? } else { read_by_entry(&path)? };
            if got != expected {
                return Err(format!("{}: {v} gives other bytes", case.name).into());
            }
        }
        let largest = largest_entry(&path)?;
        println!(
            "{}\tgrids {}\tarchive {}\tcache {}\tlargest entry {}",
            case.name,
            case.grids,
            std::fs::metadata(&path)?.len(),
            expected.len(),
            largest
        );
    }
    Ok(())
}

/// The bytes the largest entry's metadata, table and chunks take in the archive.
fn largest_entry(path: &Path) -> R<usize> {
    let bytes = std::fs::read(path)?;
    let es = directory(&bytes)?;
    let mut starts: Vec<usize> = es.iter().map(|e| e.meta_at).collect();
    starts.push(bytes.len());
    Ok(starts.windows(2).map(|w| w[1] - w[0]).max().unwrap_or(0))
}

fn threads() -> R<usize> {
    let out = std::process::Command::new("sysctl").args(["-n", "hw.perflevel0.logicalcpu"]).output()?;
    Ok(String::from_utf8(out.stdout)?.trim().parse()?)
}

fn time(rounds: usize) -> R<()> {
    let variants = ["brodo", "whole", "entry"];
    let mut evict = vec![0_u8; EVICT_BYTES];
    println!("case\tvariant\tfastest_ns\tthreads {}", rayon::current_num_threads());
    for case in &CASES {
        let path = path_of(case.name);
        let mut best = [u128::MAX; 3];
        for round in 0..rounds {
            for k in 0..variants.len() {
                let i = (k + round) % variants.len();
                for (j, b) in evict.iter_mut().enumerate() {
                    *b = b.wrapping_add(j as u8 | 1);
                }
                black_box(&evict);
                let t = Instant::now();
                black_box(read(variants[i], &path)?);
                best[i] = best[i].min(t.elapsed().as_nanos());
            }
        }
        for (v, b) in variants.iter().zip(best) {
            println!("{}\t{v}\t{b}", case.name);
        }
    }
    Ok(())
}

fn main() -> R<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    rayon::ThreadPoolBuilder::new().num_threads(threads()?).build_global()?;
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["write"] => write_cases(),
        ["time", rounds] => time(rounds.parse()?),
        ["read", case, variant] => {
            black_box(read(variant, &path_of(case))?);
            Ok(())
        }
        _ => Err("entryread-spike write | time ROUNDS | read CASE VARIANT".into()),
    }
}

