//! Format tier (TESTING.md §1) for the cache profile: a written file reads back with the same
//! metadata, bounds and values, the same grids give the same bytes, format version 1 keeps
//! the bytes of a committed file (TESTING.md V6), and damaged files are refused, never read
//! wrongly or with a panic.

use std::collections::BTreeMap;

use brodo::cache::{CacheEntry, CacheFile, to_bytes, write};
use brodo::error::{CacheError, FormatError, MetaError};
use brodo::field::{Field, Value};
use brodo::meta::{Bounds, GridMeta};
use brodo::topology::Topology;
use brodo::tree::{Coord, TileLevel};
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
    m.extra =
        extra.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect::<BTreeMap<_, _>>();
    m
}

fn coord() -> impl Strategy<Value = Coord> {
    let anchors = vec![-4097, -4096, -129, -128, -9, -8, -1, 0, 7, 8, 127, 128, 4095, 4096];
    let axis = (prop::sample::select(anchors), 0..20_i32).prop_map(|(a, d)| a + d);
    [axis.clone(), axis.clone(), axis]
}

fn level() -> impl Strategy<Value = TileLevel> {
    prop_oneof![1 => Just(TileLevel::Root), 3 => Just(TileLevel::Upper), 3 => Just(TileLevel::Lower)]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn a_file_reads_back_what_was_written(
        leaves in prop::collection::vec(coord(), 0..12),
        tiles in prop::collection::vec((level(), coord()), 0..4),
        name in "[a-zA-Zα-ω0-9 ]{0,12}",
        extra in prop::collection::btree_map("[a-z]{1,5}", "[ -~]{0,9}", 0..3),
    ) {
        let t = build(&leaves, &tiles);
        let f = numbered(&t, -1.0_f32, |k| k as f32 + 0.5);
        let g = numbered(&t, 7_u8, |k| (k % 251) as u8);
        let mut m = meta(&name, &[]);
        m.extra = extra.clone();
        let entries = [CacheEntry::new(&m, &t, &f).unwrap(), CacheEntry::new(&meta("second", &[("k", "v")]), &t, &g).unwrap()];
        let bytes = to_bytes(&entries).unwrap();
        prop_assert_eq!(&to_bytes(&entries).unwrap(), &bytes, "same grids, same bytes");
        let file = CacheFile::from_bytes(&bytes).unwrap();
        prop_assert_eq!(file.grid_count(), 2);
        prop_assert_eq!(file.meta(0).unwrap(), &m);
        prop_assert_eq!(file.bounds(0).unwrap(), Bounds::of(&t).unwrap());
        prop_assert_eq!(file.value_type(1).unwrap(), u8::TYPE_CODE);
        let (a, b) = (file.view::<f32>(0).unwrap(), file.view::<u8>(1).unwrap());
        for n in t.leaf_numbers() {
            let o = t.leaf_origin(n).unwrap();
            for v in [o, [o[0] + 7, o[1] + 3, o[2] + 5], [o[0] - 1, o[1], o[2]]] {
                prop_assert_eq!(a.get(v).unwrap().to_bits(), f.value_at(&t, v).unwrap().to_bits());
                prop_assert_eq!(b.get(v).unwrap(), g.value_at(&t, v).unwrap());
            }
        }
        let wrong = matches!(file.view::<f64>(0), Err(CacheError::Grid { index: 0, error: FormatError::WrongValueType { .. } }));
        prop_assert!(wrong, "a view of the wrong type is refused");
    }
}

#[test]
fn bounds_cover_leaves_and_tiles_and_an_empty_grid_has_none() {
    assert_eq!(Bounds::of(&Topology::new()).unwrap(), Bounds::Empty);
    let t = build(&[[-1, 0, 0]], &[(TileLevel::Upper, [128, 0, 0])]);
    assert_eq!(Bounds::of(&t).unwrap(), Bounds::Box { min: [-8, 0, 0], max: [255, 127, 127] });
}

#[test]
fn metadata_needs_a_unit_and_finite_sizes() {
    assert_eq!(GridMeta::new("g", [1.0; 3], "").map(|_| ()), Err(MetaError::EmptyUnit));
    assert_eq!(GridMeta::new("g", [1.0, 0.0, 1.0], "m").map(|_| ()), Err(MetaError::BadVoxelSize));
    let mut m = GridMeta::new("g", [1.0; 3], "m").unwrap();
    m.transform[1][2] = f64::NAN;
    let t = Topology::new();
    let f = Field::new(&t, 0.0_f32);
    assert!(matches!(CacheEntry::new(&m, &t, &f), Err(CacheError::Meta(MetaError::BadTransform))));
}

/// The grids of the committed file.
fn golden() -> Vec<u8> {
    let t = build(
        &[[-1, 0, 0], [4096, 7, 9]],
        &[(TileLevel::Lower, [8, 8, 8]), (TileLevel::Root, [0, 8192, 0])],
    );
    let f = numbered(&t, -1.0_f32, |k| k as f32 + 0.5);
    let g = numbered(&t, [0.0_f32; 3], |k| [k as f32, 1.0, -1.0]);
    let entries = [
        CacheEntry::new(&meta("density", &[("source", "test")]), &t, &f).unwrap(),
        CacheEntry::new(&meta("velocity", &[]), &t, &g).unwrap(),
    ];
    to_bytes(&entries).unwrap()
}

const GOLDEN: &[u8] = include_bytes!("data/cache_v1.bin");

#[test]
fn format_version_1_keeps_its_bytes_and_a_file_round_trips() {
    let bytes = golden();
    assert_eq!(bytes.len(), GOLDEN.len(), "length");
    assert!(
        bytes == GOLDEN,
        "the bytes of cache format version 1 changed; raise the version (TESTING.md V6)"
    );
    let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("cache_v1.brdc");
    let t = build(&[[3, 3, 3]], &[]);
    let f = numbered(&t, 0.0_f64, f64::from);
    write(&path, &[CacheEntry::new(&meta("on disk", &[]), &t, &f).unwrap()]).unwrap();
    let file = CacheFile::read(&path).unwrap();
    assert_eq!(
        file.view::<f64>(0).unwrap().get([3, 3, 3]).unwrap().to_bits(),
        f.value_at(&t, [3, 3, 3]).unwrap().to_bits()
    );
}

fn with_word(bytes: &[u8], word: usize, value: u32) -> Vec<u8> {
    let mut out = bytes.to_vec();
    out[word * 4..word * 4 + 4].copy_from_slice(&value.to_le_bytes());
    out
}

fn word(bytes: &[u8], w: usize) -> u32 {
    u32::from_le_bytes(bytes[w * 4..w * 4 + 4].try_into().unwrap())
}

#[test]
fn each_check_refuses_its_damage() {
    let g = golden();
    let read = |b: &[u8]| CacheFile::from_bytes(b).map(|_| ());
    assert!(matches!(read(&with_word(&g, 0, 9)), Err(CacheError::BadMagic(9))));
    assert!(matches!(read(&with_word(&g, 1, 2)), Err(CacheError::UnsupportedVersion(2))));
    assert!(matches!(read(&g[..g.len() - 32]), Err(CacheError::Truncated("grid"))));
    assert!(matches!(read(&g[..g.len() - 1]), Err(CacheError::Inconsistent("length"))));
    // Directory of grid 0 is words 8..16: metadata at, length, grid at, blocks, type.
    let meta_at = word(&g, 8);
    assert!(matches!(read(&with_word(&g, 10, meta_at)), Err(CacheError::Inconsistent("grid"))));
    assert!(matches!(
        read(&with_word(&g, 12, 5)),
        Err(CacheError::Grid { index: 0, error: FormatError::WrongValueType { .. } })
    ));
    // The name's first byte made invalid UTF-8.
    let mut bad = g.clone();
    bad[usize::try_from(meta_at).unwrap() * 32 + 4] = 0xff;
    assert!(matches!(read(&bad), Err(CacheError::BadText("metadata"))));
    // The unit's length made zero: what follows is read from the wrong place, and runs out.
    let unit_len = usize::try_from(meta_at).unwrap() * 8 + 3;
    assert!(matches!(read(&with_word(&g, unit_len, 0)), Err(CacheError::Truncated("metadata"))));
    // The first voxel size made zero, lengths untouched: the metadata's own check refuses it.
    // "density" takes bytes 0..12 and "mm" 12..20; the sizes follow.
    let mut zero = g.clone();
    let at = usize::try_from(meta_at).unwrap() * 32 + 20;
    zero[at..at + 8].copy_from_slice(&0.0_f64.to_le_bytes());
    assert!(matches!(read(&zero), Err(CacheError::Meta(MetaError::BadVoxelSize))));
}

/// One grid's file, its metadata from block 2.
fn one(name: &str, extra: &[(&str, &str)], leaves: &[Coord]) -> Vec<u8> {
    let t = build(leaves, &[]);
    let f = numbered(&t, 0.0_f32, |k| k as f32);
    to_bytes(&[CacheEntry::new(&meta(name, extra), &t, &f).unwrap()]).unwrap()
}

#[test]
fn only_the_bytes_the_writer_puts_are_read() {
    let g = golden();
    let read = |b: &[u8]| CacheFile::from_bytes(b).map(|_| ());
    let meta_at = usize::try_from(word(&g, 8)).unwrap() * 32;
    // Nothing after the last grid.
    let mut longer = g.clone();
    longer.extend_from_slice(&[0; 32]);
    assert!(matches!(read(&longer), Err(CacheError::Inconsistent("end"))));
    // The header's and the directory's unused words are zero.
    assert!(matches!(read(&with_word(&g, 3, 1)), Err(CacheError::Inconsistent("header"))));
    assert!(matches!(read(&with_word(&g, 13, 1)), Err(CacheError::Inconsistent("directory"))));
    // No gap: the first metadata one block later, after the directory.
    assert!(matches!(
        read(&with_word(&g, 8, word(&g, 8) + 1)),
        Err(CacheError::Inconsistent("metadata"))
    ));
    // The padding after the metadata is zero.
    let small = one("g", &[], &[[0, 0, 0]]);
    let small_end = 2 * 32 + usize::try_from(word(&small, 9)).unwrap();
    assert!(!small_end.is_multiple_of(32), "the metadata needs padding");
    let mut padded = small.clone();
    padded[small_end] = 1;
    assert!(matches!(read(&padded), Err(CacheError::Inconsistent("padding"))));
    // A string's padding: "density" is 7 bytes, padded with one.
    let mut text = g.clone();
    text[meta_at + 4 + 7] = 1;
    assert!(matches!(read(&text), Err(CacheError::Inconsistent("metadata"))));
    // A key twice, or keys out of order.
    let two = one("g", &[("ka", "1"), ("kb", "2")], &[[0, 0, 0]]);
    let at = two.windows(2).position(|w| w == b"kb").unwrap();
    let (mut twice, mut unordered) = (two.clone(), two.clone());
    twice[at + 1] = b'a';
    unordered[at + 1] = b'0';
    assert!(read(&two).is_ok());
    assert!(matches!(read(&twice), Err(CacheError::Inconsistent("metadata's keys"))));
    assert!(matches!(read(&unordered), Err(CacheError::Inconsistent("metadata"))));
    // An empty box's corners are zero: "e" and "mm" take 16 bytes, the floats 120, the
    // box's kind 4; the corners follow.
    let mut empty = one("e", &[], &[]);
    assert!(read(&empty).is_ok());
    empty[2 * 32 + 140] = 1;
    assert!(matches!(read(&empty), Err(CacheError::Inconsistent("metadata"))));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Damaged files: changed words, or a cut. Either refused, or every read answers
    /// without a panic.
    #[test]
    fn damaged_files_are_refused_or_read_safely(
        changes in prop::collection::vec((any::<prop::sample::Index>(), any::<u32>()), 1..6),
        cut in prop::option::of(any::<prop::sample::Index>()),
        early in any::<bool>(),
    ) {
        let mut bytes = golden();
        let words = if early { 64 } else { bytes.len() / 4 };
        for (i, x) in changes {
            let w = i.index(words);
            bytes[w * 4..w * 4 + 4].copy_from_slice(&x.to_le_bytes());
        }
        if let Some(c) = cut {
            bytes.truncate(c.index(bytes.len()));
        }
        if let Ok(file) = CacheFile::from_bytes(&bytes) {
            for i in 0..file.grid_count() {
                let _meta = file.meta(i);
                if let Ok(view) = file.view::<f32>(i) {
                    let _read = view.get([0, 0, 0]);
                }
            }
        }
    }
}

fn tmp(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name)
}

#[test]
#[cfg_attr(miri, ignore = "needs a real file mapping, which Miri cannot run")]
fn a_mapped_file_reads_as_a_copied_one() {
    let bytes = golden();
    let path = tmp("golden_mapped.brdc");
    std::fs::write(&path, &bytes).unwrap();
    let (mapped, copied) = (CacheFile::map(&path).unwrap(), CacheFile::from_bytes(&bytes).unwrap());
    assert_eq!(mapped.grid_count(), copied.grid_count());
    for i in 0..copied.grid_count() {
        assert_eq!(mapped.meta(i).unwrap(), copied.meta(i).unwrap());
        assert_eq!(mapped.bounds(i).unwrap(), copied.bounds(i).unwrap());
        assert_eq!(mapped.value_type(i).unwrap(), copied.value_type(i).unwrap());
    }
    let voxels = [[-1, 0, 0], [4096, 7, 9], [9, 9, 9], [0, 8200, 3], [50, 50, 50], [4100, 3, 15]];
    let (a, b) = (mapped.view::<f32>(0).unwrap(), copied.view::<f32>(0).unwrap());
    let (c, d) = (mapped.view::<[f32; 3]>(1).unwrap(), copied.view::<[f32; 3]>(1).unwrap());
    for v in voxels {
        assert_eq!(a.get(v).unwrap().to_bits(), b.get(v).unwrap().to_bits(), "at {v:?}");
        assert_eq!(
            c.get(v).unwrap().map(f32::to_bits),
            d.get(v).unwrap().map(f32::to_bits),
            "at {v:?}"
        );
    }
}

#[test]
#[cfg_attr(miri, ignore = "needs a real file mapping, which Miri cannot run")]
fn a_file_unfit_to_map_is_refused() {
    let empty = tmp("empty.brdc");
    std::fs::write(&empty, []).unwrap();
    assert!(matches!(CacheFile::map(&empty), Err(CacheError::Inconsistent("length"))));
    let odd = tmp("odd.brdc");
    std::fs::write(&odd, [0_u8; 33]).unwrap();
    assert!(matches!(CacheFile::map(&odd), Err(CacheError::Inconsistent("length"))));
    let damaged = tmp("damaged.brdc");
    std::fs::write(&damaged, with_word(&golden(), 0, 9)).unwrap();
    assert!(matches!(CacheFile::map(&damaged), Err(CacheError::BadMagic(9))));
    assert!(matches!(CacheFile::map(&tmp("missing.brdc")), Err(CacheError::Io(_))));
}
