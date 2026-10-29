//! Format tier (TESTING.md §1) for the archive profile: an archive reads back as the cache
//! file of the same grids, the same grids give the same bytes whatever the threads, format
//! version 1 keeps the bytes of a committed file (TESTING.md V6), a region reads as the
//! whole grid does within its box (§8.6), and damaged archives are refused, never read
//! wrongly or with a panic.

#![cfg(feature = "archive")]

use brodo::archive::{
    Encoding, from_bytes, read, read_region, region_from_bytes, to_bytes, to_bytes_with, write,
    write_with,
};
use brodo::cache::{self, CacheEntry, CacheFile};
use brodo::error::{CacheError, FormatError};
use brodo::field::{Field, Value};
use brodo::meta::{Bounds, GridMeta};
use brodo::topology::Topology;
use brodo::tree::{Coord, TileLevel};
use brodo::view::ViewLocation;
use proptest::prelude::*;

fn build(leaves: &[Coord], tiles: &[(TileLevel, Coord)]) -> Topology {
    let empty = Topology::new();
    let mut e = empty.edit();
    for v in leaves {
        e.activate(*v).unwrap();
    }
    for (l, v) in tiles {
        // A tile where something already is, is skipped.
        let _occupied = e.add_tile(*l, *v);
    }
    e.commit().unwrap().0
}

fn numbered<T: Value>(t: &Topology, background: T, of: impl Fn(u32) -> T) -> Field<T> {
    let mut f = Field::new(t, background);
    let mut k = 0;
    for n in t.leaf_numbers() {
        for v in f.leaf_mut(t, n).unwrap() {
            *v = of(k);
            k += 1;
        }
    }
    for n in t.tile_numbers() {
        f.set_tile(t, n, of(k)).unwrap();
        k += 1;
    }
    f
}

fn meta(name: &str, extra: &[(&str, &str)]) -> GridMeta {
    let mut m = GridMeta::new(name, [0.5, 0.5, 0.25], "mm").unwrap();
    m.transform[0][3] = -12.5;
    m.extra = extra.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect();
    m
}

/// The grids of the committed file: those of `cache_v1.bin`.
fn golden_entries() -> Vec<CacheEntry> {
    let t = build(
        &[[-1, 0, 0], [4096, 7, 9]],
        &[(TileLevel::Lower, [8, 8, 8]), (TileLevel::Root, [0, 8192, 0])],
    );
    let f = numbered(&t, -1.0_f32, |k| k as f32 + 0.5);
    let g = numbered(&t, [0.0_f32; 3], |k| [k as f32, 1.0, -1.0]);
    vec![
        CacheEntry::new(&meta("density", &[("source", "test")]), &t, &f).unwrap(),
        CacheEntry::new(&meta("velocity", &[]), &t, &g).unwrap(),
    ]
}

const GOLDEN: &[u8] = include_bytes!("data/archive_v1.bin");
const QUANTISED: &[u8] = include_bytes!("data/archive_v1_quantised.bin");

/// The golden grids, the first in binary16 and the second in 8 bits.
fn quantised(e: &[CacheEntry]) -> Vec<(&CacheEntry, Encoding)> {
    e.iter().zip([Encoding::Binary16, Encoding::EightBit]).collect()
}

/// Grids of `leaves` leaves in a row, several chunks of them for more than 512.
fn row(leaves: i32) -> Vec<CacheEntry> {
    let coords: Vec<Coord> = (0..leaves).map(|i| [8 * i, 0, 8 * (i % 5)]).collect();
    let t = build(&coords, &[(TileLevel::Upper, [0, 4096, 0])]);
    let f = numbered(&t, 0.0_f32, |k| (k % 977) as f32 * 0.25);
    let g = numbered(&t, [0.0_f64; 3], |k| [f64::from(k), 0.0, -1.0]);
    vec![
        CacheEntry::new(&meta("f", &[]), &t, &f).unwrap(),
        CacheEntry::new(&meta("v", &[("k", "v")]), &t, &g).unwrap(),
    ]
}

/// Two files read alike: every grid's metadata, bounds, type and values at its leaves.
fn same(a: &CacheFile, b: &CacheFile, t: &Topology) {
    assert_eq!(a.grid_count(), b.grid_count());
    for i in 0..a.grid_count() {
        assert_eq!(a.meta(i).unwrap(), b.meta(i).unwrap());
        assert_eq!(a.bounds(i).unwrap(), b.bounds(i).unwrap());
        assert_eq!(a.value_type(i).unwrap(), b.value_type(i).unwrap());
    }
    let (x, y) = (a.view::<f32>(0).unwrap(), b.view::<f32>(0).unwrap());
    for n in t.leaf_numbers() {
        let o = t.leaf_origin(n).unwrap();
        for v in [o, [o[0] + 7, o[1] + 3, o[2] + 5], [o[0] - 1, o[1], o[2]]] {
            assert_eq!(x.get(v).unwrap().to_bits(), y.get(v).unwrap().to_bits());
        }
    }
}

#[test]
fn an_archive_reads_as_the_cache_file_of_the_same_grids() {
    for leaves in [0, 1, 512, 513, 1300] {
        let e = row(leaves);
        let coords: Vec<Coord> = (0..leaves).map(|i| [8 * i, 0, 8 * (i % 5)]).collect();
        let t = build(&coords, &[(TileLevel::Upper, [0, 4096, 0])]);
        let archived = from_bytes(&to_bytes(&e).unwrap(), u64::MAX).unwrap();
        let cached = CacheFile::from_bytes(&cache::to_bytes(&e).unwrap()).unwrap();
        same(&archived, &cached, &t);
    }
}

/// Chunks are compressed in parallel, and the bytes are the same on one thread or many
/// (ARCHITECTURE.md §2's reproducibility).
#[test]
fn the_same_grids_give_the_same_bytes_whatever_the_threads() {
    let e = row(1300);
    let on = |n: usize| {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(n).build().unwrap();
        pool.install(|| to_bytes(&e).unwrap())
    };
    let one = on(1);
    assert_eq!(on(8), one);
    let file = |n: usize| {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(n).build().unwrap();
        pool.install(|| from_bytes(&one, u64::MAX).unwrap())
    };
    let coords: Vec<Coord> = (0..1300).map(|i| [8 * i, 0, 8 * (i % 5)]).collect();
    same(&file(1), &file(8), &build(&coords, &[(TileLevel::Upper, [0, 4096, 0])]));
}

#[test]
fn format_version_1_keeps_its_bytes_and_a_file_round_trips() {
    let bytes = to_bytes(&golden_entries()).unwrap();
    assert_eq!(bytes.len(), GOLDEN.len(), "length");
    assert!(
        bytes == GOLDEN,
        "the bytes of archive format version 1 changed; raise the version (TESTING.md V6)"
    );
    let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("archive_v1.brda");
    write(&path, &golden_entries()).unwrap();
    let cached = CacheFile::from_bytes(&cache::to_bytes(&golden_entries()).unwrap()).unwrap();
    let t = build(
        &[[-1, 0, 0], [4096, 7, 9]],
        &[(TileLevel::Lower, [8, 8, 8]), (TileLevel::Root, [0, 8192, 0])],
    );
    same(&read(&path, u64::MAX).unwrap(), &cached, &t);
}

fn with_word(bytes: &[u8], word: usize, value: u32) -> Vec<u8> {
    let mut out = bytes.to_vec();
    out[word * 4..word * 4 + 4].copy_from_slice(&value.to_le_bytes());
    out
}

fn word(bytes: &[u8], w: usize) -> u32 {
    u32::from_le_bytes(bytes[w * 4..w * 4 + 4].try_into().unwrap())
}

/// A file of no grid is its header alone (docs/formats.md §8), and is read as such.
#[test]
fn a_file_of_no_grid_is_its_header() {
    let bytes = to_bytes(&[]).unwrap();
    assert_eq!(bytes.len(), 32);
    assert_eq!(from_bytes(&bytes, u64::MAX).unwrap().grid_count(), 0);
    let mut longer = bytes.clone();
    longer.extend([0; 32]);
    assert!(matches!(from_bytes(&longer, u64::MAX), Err(CacheError::Inconsistent("end"))));
}

/// A cache file larger than the limit is refused by the lengths the archive states, before
/// it is made; a file longer than it, before it is read (ADR-0036).
#[test]
fn the_limit_bounds_the_file_and_what_it_decompresses_into() {
    let g = to_bytes(&golden_entries()).unwrap();
    let made = cache::to_bytes(&golden_entries()).unwrap().len() as u64;
    assert!(made > g.len() as u64, "the golden file compresses");
    assert!(from_bytes(&g, made).is_ok());
    assert!(matches!(
        from_bytes(&g, made - 1),
        Err(CacheError::FileTooLarge { length, limit }) if length == made && limit == made - 1
    ));
    // A frozen buffer said to be 2²⁴ blocks, 512 MiB, its first chunk the more: refused by
    // the limit, not made.
    let table = word(&g, 10) as usize * 8;
    let first = (1 << 24) - (word(&g, 12) - word(&g, table));
    let huge = with_word(&with_word(&g, 12, 1 << 24), table, first);
    assert!(matches!(from_bytes(&huge, 1 << 20), Err(CacheError::FileTooLarge { .. })));
    let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("archive_limit.brda");
    std::fs::write(&path, &g).unwrap();
    assert!(matches!(
        read(&path, g.len() as u64 - 1),
        Err(CacheError::FileTooLarge { length, .. }) if length == g.len() as u64
    ));
}

#[test]
fn each_check_refuses_its_damage() {
    let g = GOLDEN.to_vec();
    let read = |b: &[u8]| from_bytes(b, u64::MAX).map(|_| ());
    let cache_file = cache::to_bytes(&golden_entries()).unwrap();
    assert!(matches!(read(&cache_file), Err(CacheError::BadMagic(_))));
    assert!(matches!(read(&with_word(&g, 1, 2)), Err(CacheError::UnsupportedVersion(2))));
    assert!(matches!(read(&with_word(&g, 3, 2)), Err(CacheError::Inconsistent("header"))));
    assert!(matches!(read(&with_word(&g, 4, 256)), Err(CacheError::Inconsistent("header"))));
    assert!(matches!(read(&with_word(&g, 5, 1)), Err(CacheError::Inconsistent("header"))));
    assert!(matches!(read(&g[..g.len() - 32]), Err(CacheError::Truncated(_))));
    assert!(matches!(read(&g[..g.len() - 1]), Err(CacheError::Inconsistent("length"))));
    // Directory of grid 0 is words 8..16: metadata at, length, table at, chunks, blocks,
    // type, encoding, then one unused.
    assert!(matches!(read(&with_word(&g, 15, 1)), Err(CacheError::Inconsistent("directory"))));
    assert!(matches!(read(&with_word(&g, 14, 3)), Err(CacheError::UnknownEncoding(3))));
    // Exact chunks said to be binary16: the first leaf chunk does not decompress to half
    // its length.
    assert!(matches!(
        read(&with_word(&g, 14, 1)),
        Err(CacheError::Decompress { index: 0, chunk: 1 })
    ));
    // Eight bits want ranges after the table, which then holds what is not one.
    assert!(read(&with_word(&g, 14, 2)).is_err());
    assert!(matches!(read(&with_word(&g, 8, 4)), Err(CacheError::Inconsistent("metadata"))));
    assert!(matches!(
        read(&with_word(&g, 10, word(&g, 10) + 1)),
        Err(CacheError::Inconsistent("chunk table"))
    ));
    assert!(matches!(
        read(&with_word(&g, 12, word(&g, 12) + 1)),
        Err(CacheError::Inconsistent("chunk table"))
    ));
    assert!(matches!(
        read(&with_word(&g, 13, 5)),
        Err(CacheError::Grid { index: 0, error: FormatError::WrongValueType { .. } })
    ));
    // Grid 0's first chunk said one block longer before compression, its second one
    // shorter: they add up, and neither decompresses to its length.
    let table = word(&g, 10) as usize * 8;
    let (first, second, chunks) = (word(&g, table), word(&g, table + 2), word(&g, 11) as usize);
    let moved = with_word(&with_word(&g, table, first + 1), table + 2, second - 1);
    assert!(matches!(read(&moved), Err(CacheError::Decompress { index: 0, chunk: 0 })));
    // A chunk of no blocks.
    assert!(matches!(read(&with_word(&g, table, 0)), Err(CacheError::Inconsistent("chunk table"))));
    // The padding after the chunk table is zero.
    let pad = table * 4 + chunks * 8;
    assert!(!pad.is_multiple_of(32), "the table needs padding");
    let mut padded = g.clone();
    padded[pad] = 1;
    assert!(matches!(read(&padded), Err(CacheError::Inconsistent("padding"))));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Damaged archives, exact or quantised: changed words, or a cut. Either refused, or
    /// every read answers without a panic.
    #[test]
    fn damaged_files_are_refused_or_read_safely(
        changes in prop::collection::vec((any::<prop::sample::Index>(), any::<u32>()), 1..6),
        cut in prop::option::of(any::<prop::sample::Index>()),
        early in any::<bool>(),
        quantised in any::<bool>(),
    ) {
        let mut bytes = if quantised { QUANTISED } else { GOLDEN }.to_vec();
        let words = if early { 64 } else { bytes.len() / 4 };
        for (i, x) in changes {
            let w = i.index(words);
            bytes[w * 4..w * 4 + 4].copy_from_slice(&x.to_le_bytes());
        }
        if let Some(c) = cut {
            bytes.truncate(c.index(bytes.len()));
        }
        if let Ok(file) = from_bytes(&bytes, 1 << 24) {
            for i in 0..file.grid_count() {
                let _meta = file.meta(i);
                if let Ok(view) = file.view::<f32>(i) {
                    let _read = view.get([0, 0, 0]);
                }
            }
        }
        let extent = Bounds::Box { min: [-16; 3], max: [16; 3] };
        if let Ok(region) = region_from_bytes(&bytes, 1 << 24, 0, extent)
            && let Ok(view) = region.view::<f32>()
        {
            let _read = view.get([0, 0, 0]);
            for (n, o) in view.leaves() {
                let _leaf = view.leaf(n);
                let _value = view.get(o);
            }
        }
    }
}

/// Quantised grids keep their bytes (TESTING.md V6), and read back within their bounds
/// (docs/formats.md §8.5): binary16 within 2⁻¹¹ × |x| + 2⁻²⁵, 8 bits within a 510th of
/// the range; the background and the tiles exact.
#[test]
fn quantised_values_keep_their_bytes_and_read_back_within_their_bounds() {
    let e = golden_entries();
    let bytes = to_bytes_with(&quantised(&e)).unwrap();
    assert!(bytes == QUANTISED, "the bytes of a quantised archive changed (TESTING.md V6)");
    assert!(bytes.len() < GOLDEN.len(), "quantised is smaller");
    let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("archive_v1_q.brda");
    write_with(&path, &quantised(&e)).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let file = read(&path, u64::MAX).unwrap();
    let exact = CacheFile::from_bytes(&cache::to_bytes(&e).unwrap()).unwrap();
    let t = build(
        &[[-1, 0, 0], [4096, 7, 9]],
        &[(TileLevel::Lower, [8, 8, 8]), (TileLevel::Root, [0, 8192, 0])],
    );
    let (a, x) = (file.view::<f32>(0).unwrap(), exact.view::<f32>(0).unwrap());
    let (b, y) = (file.view::<[f32; 3]>(1).unwrap(), exact.view::<[f32; 3]>(1).unwrap());
    let step = 1023.0 / 255.0;
    for n in t.leaf_numbers() {
        let o = t.leaf_origin(n).unwrap();
        for i in 0..512 {
            let v = [o[0] + i % 8, o[1] + i / 8 % 8, o[2] + i / 64];
            let (got, want) = (a.get(v).unwrap(), x.get(v).unwrap());
            assert!(
                (got - want).abs() <= want.abs() / 2048.0 + 2.0_f32.powi(-25),
                "{want} as {got}"
            );
            let (got, want) = (b.get(v).unwrap(), y.get(v).unwrap());
            for (g, w) in got.iter().zip(want) {
                assert!((g - w).abs() <= step / 2.0 * 1.000_001, "{w} as {g}");
            }
        }
    }
    // Outside the leaves: a tile and the background, exact.
    for v in [[8, 8, 8], [0, 8192, 0], [-4096, 0, 0]] {
        assert_eq!(a.get(v).unwrap().to_bits(), x.get(v).unwrap().to_bits());
        assert_eq!(b.get(v).unwrap().map(f32::to_bits), y.get(v).unwrap().map(f32::to_bits));
    }
}

/// A grid quantised but not of a floating type, and a value its encoding cannot hold, are
/// refused at write, naming the grid.
#[test]
fn a_value_its_encoding_cannot_hold_is_refused() {
    let t = build(&[[0, 0, 0]], &[]);
    let m = meta("g", &[]);
    let bytes = CacheEntry::new(&m, &t, &Field::new(&t, 3_u8)).unwrap();
    let exact = CacheEntry::new(&m, &t, &Field::new(&t, 1.0_f32)).unwrap();
    for encoding in [Encoding::Binary16, Encoding::EightBit] {
        assert!(matches!(
            to_bytes_with(&[(&exact, encoding), (&bytes, encoding)]),
            Err(CacheError::NotQuantised { index: 1, value_type: 3 })
        ));
    }
    let with = |x: f64| {
        let mut f = Field::new(&t, [0.0_f64; 3]);
        f.leaf_mut(&t, t.leaf_numbers().next().unwrap()).unwrap()[7] = [0.0, x, 0.0];
        CacheEntry::new(&m, &t, &f).unwrap()
    };
    for (x, encoding) in [
        (65504.5, Encoding::Binary16),
        (-1.0e6, Encoding::Binary16),
        (f64::NAN, Encoding::Binary16),
        (f64::NAN, Encoding::EightBit),
        (f64::INFINITY, Encoding::EightBit),
    ] {
        let g = with(x);
        assert!(
            matches!(
                to_bytes_with(&[(&exact, Encoding::Exact), (&g, encoding)]),
                Err(CacheError::Unrepresentable(1))
            ),
            "{x} in {encoding:?}"
        );
    }
    // Wider than an f64 holds: the range from -f64::MAX to f64::MAX.
    let mut f = Field::new(&t, [0.0_f64; 3]);
    let leaf = f.leaf_mut(&t, t.leaf_numbers().next().unwrap()).unwrap();
    leaf[0] = [f64::MAX; 3];
    leaf[1] = [-f64::MAX; 3];
    let g = CacheEntry::new(&m, &t, &f).unwrap();
    assert!(matches!(
        to_bytes_with(&[(&g, Encoding::EightBit)]),
        Err(CacheError::Unrepresentable(0))
    ));
    assert!(to_bytes_with(&[(&with(65504.0), Encoding::Binary16)]).is_ok());
}

/// Quantised chunks are encoded in parallel, and the bytes are the same on one thread or
/// many; binary16 read back and written again gives the same bytes.
#[test]
fn quantised_grids_give_the_same_bytes_whatever_the_threads() {
    let e = row(1300);
    let grids = quantised(&e);
    let on = |n: usize| {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(n).build().unwrap();
        pool.install(|| to_bytes_with(&grids).unwrap())
    };
    let one = on(1);
    assert_eq!(on(8), one);
    let back = from_bytes(&one, u64::MAX).unwrap();
    let (t, f) = back.view::<f32>(0).unwrap().thaw().unwrap();
    let again = CacheEntry::new(back.meta(0).unwrap(), &t, &f).unwrap();
    let first = to_bytes_with(&[(&e[0], Encoding::Binary16)]).unwrap();
    assert_eq!(to_bytes_with(&[(&again, Encoding::Binary16)]).unwrap(), first);
}

/// An 8-bit grid's ranges are checked: finite, the least at or below the greatest.
#[test]
fn an_eight_bit_grids_ranges_are_checked() {
    let q = QUANTISED.to_vec();
    let read = |b: &[u8]| from_bytes(b, u64::MAX).map(|_| ());
    assert!(read(&q).is_ok());
    // Grid 1's directory is words 16..24; its ranges follow its table's entries.
    let ranges = word(&q, 18) as usize * 8 + word(&q, 19) as usize * 2;
    let lo = f64::from_le_bytes(q[ranges * 4..ranges * 4 + 8].try_into().unwrap());
    let hi = f64::from_le_bytes(q[ranges * 4 + 8..ranges * 4 + 16].try_into().unwrap());
    assert!(lo < hi, "the x component spans {lo}..{hi}");
    for bad in [f64::NAN, f64::INFINITY, hi + 1.0] {
        let mut b = q.clone();
        b[ranges * 4..ranges * 4 + 8].copy_from_slice(&bad.to_le_bytes());
        assert!(matches!(read(&b), Err(CacheError::Inconsistent("chunk table"))), "lo {bad}");
    }
    // Ranges said to be binary16's: the table is then shorter than its part, the padding
    // not zero.
    assert!(matches!(read(&with_word(&q, 22, 1)), Err(CacheError::Inconsistent("padding"))));
}

/// The leaves of `row`'s grids, their numbers being the frozen buffer's (docs/formats.md §2).
fn row_topology(leaves: i32) -> Topology {
    let coords: Vec<Coord> = (0..leaves).map(|i| [8 * i, 0, 8 * (i % 5)]).collect();
    build(&coords, &[(TileLevel::Upper, [0, 4096, 0])])
}

/// Whether a cube of `edge` voxels at `o` meets the box from `min` to `max`.
fn meets(o: Coord, edge: i32, min: Coord, max: Coord) -> bool {
    (0..3).all(|k| o[k] <= max[k] && o[k] + edge > min[k])
}

/// A region answers every voxel within its box as the whole grid does, bit for bit,
/// quantised or not, lists the leaves meeting it, and refuses a voxel outside it and a leaf
/// not meeting it (docs/formats.md §8.6).
#[test]
fn a_region_answers_within_its_box_as_the_whole_grid_does() {
    let e = row(1300);
    let t = row_topology(1300);
    let boxes = [
        ([0, 0, 0], [0, 0, 0]),
        ([4085, -3, 0], [4100, 0, 40]),
        ([-100, -100, -100], [20000, 100, 100]),
        ([0, 4100, 5], [3, 4200, 9]),
        ([-50, 0, 0], [-10, 9, 9]),
        ([9, 1, 9], [9, 1, 9]),
    ];
    for encoding in [Encoding::Exact, Encoding::Binary16, Encoding::EightBit] {
        let bytes = to_bytes_with(&[(&e[0], encoding), (&e[1], Encoding::Exact)]).unwrap();
        let whole = from_bytes(&bytes, u64::MAX).unwrap();
        for (min, max) in boxes {
            same_within(&bytes, &whole, &t, min, max);
        }
        let region =
            region_from_bytes(&bytes, u64::MAX, 1, Bounds::Box { min: [0; 3], max: [9; 3] });
        let v = region.unwrap();
        let (r, w) = (v.view::<[f64; 3]>().unwrap(), whole.view::<[f64; 3]>(1).unwrap());
        assert_eq!(
            r.get([8, 0, 8]).unwrap().map(f64::to_bits),
            w.get([8, 0, 8]).unwrap().map(f64::to_bits)
        );
    }
    let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("archive_region.brda");
    write(&path, &e).unwrap();
    let extent = Bounds::Box { min: [0; 3], max: [7; 3] };
    let r = read_region(&path, u64::MAX, 0, extent).unwrap();
    assert_eq!(r.view::<f32>().unwrap().leaves().count(), 1);
    assert!(matches!(read_region(&path, 31, 0, extent), Err(CacheError::FileTooLarge { .. })));
}

/// The region `min` to `max` of grid 0 of `bytes`, whose whole read is `whole` and whose
/// topology is `t`, answers as `whole` does within it and refuses what lies outside.
fn same_within(bytes: &[u8], whole: &CacheFile, t: &Topology, min: Coord, max: Coord) {
    let w = whole.view::<f32>(0).unwrap();
    let region = region_from_bytes(bytes, u64::MAX, 0, Bounds::Box { min, max }).unwrap();
    assert_eq!(region.meta(), whole.meta(0).unwrap());
    assert_eq!(region.bounds(), whole.bounds(0).unwrap());
    assert_eq!(region.extent(), Bounds::Box { min, max });
    let r = region.view::<f32>().unwrap();
    let want: Vec<(u32, Coord)> = t
        .leaf_numbers()
        .map(|n| (n.get(), t.leaf_origin(n).unwrap()))
        .filter(|(_, o)| meets(*o, 8, min, max))
        .collect();
    assert_eq!(r.leaves().collect::<Vec<_>>(), want, "{min:?}");
    let bits = |l: &[f32; 512]| l.map(f32::to_bits);
    for (n, _) in &want {
        assert_eq!(bits(r.leaf(*n).unwrap()), bits(w.leaf(*n).unwrap()));
    }
    let mut voxels = vec![min, max, [min[0], max[1], min[2]]];
    voxels.extend(want.iter().map(|(_, o)| o.map(|c| c + 3)));
    for v in voxels.into_iter().filter(|v| (0..3).all(|k| min[k] <= v[k] && v[k] <= max[k])) {
        assert_eq!(r.locate(v).unwrap(), w.locate(v).unwrap(), "at {v:?}");
        assert_eq!(r.get(v).unwrap().to_bits(), w.get(v).unwrap().to_bits(), "at {v:?}");
    }
    for v in [[min[0] - 1, min[1], min[2]], [max[0], max[1], max[2] + 1]] {
        assert_eq!(r.get(v), Err(FormatError::OutsideRegion), "at {v:?}");
    }
    let away = t.leaf_numbers().map(|n| n.get()).find(|n| want.iter().all(|(m, _)| m != n));
    if let Some(n) = away {
        assert_eq!(r.leaf(n), Err(FormatError::OutsideRegion));
    }
    assert!(matches!(
        region.view::<u8>(),
        Err(CacheError::Grid { index: 0, error: FormatError::WrongValueType { .. } })
    ));
}

/// The byte range of chunk `k` of the grid whose directory entry is at word `dir`, in an
/// exact archive.
fn chunk_at(bytes: &[u8], dir: usize, k: usize) -> std::ops::Range<usize> {
    let table = word(bytes, dir + 2) as usize * 32;
    let count = word(bytes, dir + 3) as usize;
    let mut at = (table + 8 * count).next_multiple_of(32);
    for j in 0..k {
        at = (at + word(bytes, table / 4 + 2 * j + 1) as usize).next_multiple_of(32);
    }
    at..at + word(bytes, table / 4 + 2 * k + 1) as usize
}

/// A region holds and checks only its grid's first chunk, its tile chunk and the leaf
/// chunks it needs: the limit bounds those alone, and a chunk it does not decompress is not
/// checked, though a whole read refuses it (docs/formats.md §8.6).
#[test]
fn a_region_holds_and_checks_only_the_chunks_it_needs() {
    let e = row(1300);
    let bytes = to_bytes(&e).unwrap();
    // Grid 0: the nodes, three chunks of leaves, the tiles.
    assert_eq!(word(&bytes, 11), 5);
    let table = word(&bytes, 10) as usize * 8;
    let raw = |k: usize| u64::from(word(&bytes, table + 2 * k)) * 32;
    let near = Bounds::Box { min: [0; 3], max: [7; 3] };
    let held = raw(0) + raw(1) + raw(4);
    assert!(region_from_bytes(&bytes, held, 0, near).is_ok());
    assert!(matches!(
        region_from_bytes(&bytes, held - 1, 0, near),
        Err(CacheError::FileTooLarge { length, .. }) if length == held
    ));
    let mut broken = bytes.clone();
    broken[chunk_at(&bytes, 8, 3)].fill(0xff);
    assert!(matches!(
        from_bytes(&broken, u64::MAX),
        Err(CacheError::Decompress { index: 0, chunk: 3 })
    ));
    let r = region_from_bytes(&broken, u64::MAX, 0, near).unwrap();
    assert_eq!(r.view::<f32>().unwrap().get([1, 0, 0]).unwrap().to_bits(), 0.25_f32.to_bits());
    // Leaf 1299, at z = 8 × (1299 mod 5).
    let far = Bounds::Box { min: [8 * 1299, 0, 32], max: [8 * 1299, 0, 32] };
    assert!(matches!(
        region_from_bytes(&broken, u64::MAX, 0, far),
        Err(CacheError::Decompress { index: 0, chunk: 3 })
    ));
}

/// A region's read checks the archive's structure and its grid's first chunk as a whole
/// read does, and refuses a grid the archive does not hold; a box of no voxel holds no leaf
/// and answers nothing.
#[test]
fn a_region_checks_the_archives_structure() {
    let g = GOLDEN.to_vec();
    let extent = Bounds::Box { min: [0; 3], max: [9; 3] };
    let region = |b: &[u8]| region_from_bytes(b, u64::MAX, 0, extent).map(|_| ());
    assert!(region(&g).is_ok());
    assert!(matches!(region_from_bytes(&g, u64::MAX, 2, extent), Err(CacheError::NoGrid(2))));
    assert!(matches!(
        region(&cache::to_bytes(&golden_entries()).unwrap()),
        Err(CacheError::BadMagic(_))
    ));
    assert!(matches!(region(&with_word(&g, 4, 256)), Err(CacheError::Inconsistent("header"))));
    assert!(matches!(region(&with_word(&g, 15, 1)), Err(CacheError::Inconsistent("directory"))));
    assert!(matches!(region(&g[..g.len() - 32]), Err(CacheError::Truncated(_))));
    assert!(matches!(
        region(&with_word(&g, 13, 5)),
        Err(CacheError::Grid { index: 0, error: FormatError::WrongValueType { .. } })
    ));
    // Grid 1's damage is found by grid 0's region: its padding is checked.
    let table = word(&g, 18) as usize * 32 + word(&g, 19) as usize * 8;
    let mut padded = g.clone();
    padded[table] = 1;
    assert!(matches!(region(&padded), Err(CacheError::Inconsistent("padding"))));
    let empty = region_from_bytes(&g, u64::MAX, 0, Bounds::Empty).unwrap();
    let view = empty.view::<f32>().unwrap();
    assert_eq!(view.leaves().count(), 0);
    assert_eq!(view.locate([0, 0, 0]), Err(FormatError::OutsideRegion));
    let full = region_from_bytes(&g, u64::MAX, 0, Bounds::Box { min: [-9; 3], max: [9; 3] });
    let full = full.unwrap();
    let r = full.view::<f32>().unwrap();
    assert_eq!(
        r.locate([8, 8, 8]).unwrap(),
        from_bytes(&g, u64::MAX).unwrap().view::<f32>(0).unwrap().locate([8, 8, 8]).unwrap()
    );
    assert!(matches!(r.locate([8, 8, 8]).unwrap(), ViewLocation::Tile(..)));
    assert_eq!(r.background().to_bits(), (-1.0_f32).to_bits());
}
