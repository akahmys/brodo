//! Format tier (TESTING.md §1) for the cache profile: a written file reads back with the same
//! metadata, bounds and values, the same grids give the same bytes, the format keeps the
//! bytes of a committed file (TESTING.md V6), and damaged files are refused, never read
//! wrongly or with a panic.

#![expect(
    unsafe_code,
    reason = "mapping a cache file is unsafe: the test keeps its files unchanged while mapped (ADR-0036)"
)]

use std::collections::BTreeMap;

use brodo::cache::{CacheEntry, CacheFile, to_bytes, write};
use brodo::error::{CacheError, FormatError, MetaError};
use brodo::field::{AnyField, Field, Value, ValueType};
use brodo::meta::{Bounds, GridMeta};
use brodo::topology::Topology;
use brodo::tree::{Coord, TileLevel};
use proptest::prelude::*;

#[path = "support/anchored.rs"]
mod anchored;

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

fn level() -> impl Strategy<Value = TileLevel> {
    prop_oneof![1 => Just(TileLevel::Root), 3 => Just(TileLevel::Upper), 3 => Just(TileLevel::Lower)]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn a_file_reads_back_what_was_written(
        leaves in prop::collection::vec(anchored::coord(20), 0..12),
        tiles in prop::collection::vec((level(), anchored::coord(20)), 0..4),
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
        let file = brodo::cache::from_bytes(&bytes).unwrap();
        prop_assert_eq!(file.grids().len(), 2);
        prop_assert_eq!(file.grids()[0].meta(), &m);
        prop_assert_eq!(file.grids()[0].bounds(), Bounds::of(&t).unwrap());
        prop_assert_eq!(file.grids()[1].value_type(), ValueType::U8);
        let (a, b) = (file.grids()[0].view::<f32>().unwrap(), file.grids()[1].view::<u8>().unwrap());
        for n in t.leaf_numbers() {
            let o = t.leaf_origin(n).unwrap();
            for v in [o, [o[0] + 7, o[1] + 3, o[2] + 5], [o[0] - 1, o[1], o[2]]] {
                prop_assert_eq!(a.get(v).unwrap().to_bits(), f.value_at(&t, v).unwrap().to_bits());
                prop_assert_eq!(b.get(v).unwrap(), g.value_at(&t, v).unwrap());
            }
        }
        let wrong = matches!(file.grids()[0].view::<f64>(), Err(CacheError::Grid { index: 0, error: FormatError::WrongValueType { .. } }));
        prop_assert!(wrong, "a view of the wrong type is refused");
    }
}

#[test]
fn bounds_cover_leaves_and_tiles_and_an_empty_grid_has_none() {
    assert_eq!(Bounds::of(&Topology::new()).unwrap(), Bounds::Empty);
    let t = build(&[[-1, 0, 0]], &[(TileLevel::Upper, [128, 0, 0])]);
    assert_eq!(Bounds::of(&t).unwrap(), Bounds::Box { min: [-8, 0, 0], max: [255, 127, 127] });
}

/// A file's grids are listed in its order, numbered among themselves past its entries of
/// bytes, each handle naming its number in the errors it gives; names need not be unique
/// (ADR-0060).
#[test]
fn grids_are_listed_in_order_past_the_entries_of_bytes() {
    let t = build(&[[0, 0, 0]], &[]);
    let entries = [
        CacheEntry::bytes("a", BTreeMap::new(), vec![1, 2, 3]).unwrap(),
        CacheEntry::new(&meta("first", &[]), &t, &Field::new(&t, 1.0_f32)).unwrap(),
        CacheEntry::bytes("first", BTreeMap::new(), vec![4]).unwrap(),
        CacheEntry::new(&meta("first", &[]), &t, &Field::new(&t, 2_u8)).unwrap(),
    ];
    let file = brodo::cache::from_bytes(&to_bytes(&entries).unwrap()).unwrap();
    let grids = file.grids();
    let names: Vec<_> = grids.iter().map(|g| (g.index(), g.meta().name.as_str())).collect();
    assert_eq!(names, [(0, "first"), (1, "first")]);
    assert_eq!((grids[0].value_type(), grids[1].value_type()), (ValueType::F32, ValueType::U8));
    assert_eq!(grids[1].bounds(), Bounds::Box { min: [0; 3], max: [7; 3] });
    assert_eq!(grids[1].view::<u8>().unwrap().get([1, 2, 3]).unwrap(), 2);
    assert!(matches!(
        grids[1].view::<f32>(),
        Err(CacheError::Grid { index: 1, error: FormatError::WrongValueType { .. } })
    ));
    let bytes: Vec<_> = file.bytes_entries().iter().map(|b| (b.name, b.bytes.to_vec())).collect();
    assert_eq!(bytes, [("a", vec![1, 2, 3]), ("first", vec![4])]);
}

/// Keys beginning `brodo.` are brodo's formats' (U1 of P5's review, ADR-0053): refused by
/// `new` and `bytes`, so that a grid is not taken for a Hermite set's by mistake, and taken
/// by their `with_reserved_keys` forms, which a set's own entries and files made to break a
/// format use; the SDF convention's key is one `new` takes, and a key only beginning
/// `brodo`, or beginning `hermite.`, an earlier version's, is free.
#[test]
fn a_brodo_key_is_written_on_purpose_only() {
    let t = Topology::new();
    let f = Field::new(&t, 0.0_f32);
    for key in ["brodo.kind", "brodo.hermite.set"] {
        let m = meta("g", &[(key, "x")]);
        assert!(matches!(CacheEntry::new(&m, &t, &f), Err(CacheError::ReservedKey(k)) if k == key));
        assert!(CacheEntry::with_reserved_keys(&m, &t, &f).is_ok());
    }
    for key in ["brodosity", "hermite.set", brodo::meta::SDF_KEY] {
        assert!(CacheEntry::new(&meta("g", &[(key, "x")]), &t, &f).is_ok());
    }
    let extra = BTreeMap::from([("brodo.kind".to_owned(), "x".to_owned())]);
    assert!(matches!(
        CacheEntry::bytes("b", extra.clone(), vec![1]),
        Err(CacheError::ReservedKey(k)) if k == "brodo.kind"
    ));
    let free = BTreeMap::from([("brodosity".to_owned(), "x".to_owned())]);
    assert!(CacheEntry::bytes("b", free, vec![1]).is_ok());
    let file = to_bytes(&[CacheEntry::bytes_with_reserved_keys("b", extra, vec![1])]).unwrap();
    let read = brodo::cache::from_bytes(&file).unwrap();
    assert_eq!(read.bytes_entries()[0].extra.get("brodo.kind").map(String::as_str), Some("x"));
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
/// The committed file's grids: two, `f32` and `[f32; 3]`, on leaves and a tile.
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

/// The bytes of a file of the committed file's grids alone.
fn golden() -> Vec<u8> {
    to_bytes(&golden_entries()).unwrap()
}

const GOLDEN_2: &[u8] = include_bytes!("data/cache_v2.bin");

/// The committed file of version 2: the same grids, and an entry of bytes between them.
fn golden_entries_v2() -> Vec<CacheEntry> {
    let mut e = golden_entries();
    let extra = BTreeMap::from([("kind".to_owned(), "notes".to_owned())]);
    e.insert(1, CacheEntry::bytes("readme", extra, b"brodo, 2026".to_vec()).unwrap());
    e
}

/// Version 2 keeps its bytes: its grids' frozen buffers of layout version 2, an entry of
/// bytes in its place in the directory, its bytes read back (docs/formats.md §3).
#[test]
fn format_version_2_keeps_its_bytes() {
    let bytes = to_bytes(&golden_entries_v2()).unwrap();
    assert!(bytes == GOLDEN_2, "the bytes of cache format version 2 changed; raise the version");
    let file = brodo::cache::from_bytes(GOLDEN_2).unwrap();
    assert_eq!((file.version(), file.grids().len()), (2, 2));
    let entries = file.bytes_entries();
    assert_eq!(entries.len(), 1);
    assert_eq!((entries[0].name, entries[0].bytes), ("readme", &b"brodo, 2026"[..]));
    assert_eq!(entries[0].extra.get("kind").map(String::as_str), Some("notes"));
    assert_eq!(file.grids()[1].meta().name, "velocity");
    let one = with_word(GOLDEN_2, 1, 1);
    assert!(matches!(brodo::cache::from_bytes(&one), Err(CacheError::UnsupportedVersion(1))));
}

/// An entry of bytes is padded to the next block and no further (docs/formats.md §3): its
/// length stated a block short of its payload is refused, a byte short is read.
#[test]
fn an_entry_of_bytes_padded_a_whole_block_is_refused() {
    let file =
        to_bytes(&[CacheEntry::bytes("zeros", BTreeMap::new(), vec![0; 64]).unwrap()]).unwrap();
    let name = file.windows(5).position(|w| w == b"zeros").unwrap();
    let at = name + file[name..].windows(8).position(|w| w == 64u64.to_le_bytes()).unwrap();
    let with_length = |n: u64| {
        let mut out = file.clone();
        out[at..at + 8].copy_from_slice(&n.to_le_bytes());
        out
    };
    assert_eq!(
        brodo::cache::from_bytes(&with_length(33)).unwrap().bytes_entries()[0].bytes.len(),
        33
    );
    assert!(matches!(
        brodo::cache::from_bytes(&with_length(32)),
        Err(CacheError::Inconsistent("entry of bytes"))
    ));
}

/// A file is replaced whole, not written in place (G1 of P5's review): a handle opened on
/// the old file reads the old bytes after a new file is written over it, and nothing is
/// left beside it; a write that cannot finish leaves the old file.
#[test]
fn a_file_is_replaced_whole() {
    use std::io::Read;
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("replaced_whole");
    let _fresh = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("grids.brdc");
    let t = build(&[[3, 3, 3]], &[]);
    let entry = |x: f32| CacheEntry::new(&meta("g", &[]), &t, &Field::new(&t, x)).unwrap();
    write(&path, &[entry(1.0)]).unwrap();
    let old = std::fs::read(&path).unwrap();
    let mut held = std::fs::File::open(&path).unwrap();
    write(&path, &[entry(2.0)]).unwrap();
    let mut seen = Vec::new();
    held.read_to_end(&mut seen).unwrap();
    assert_eq!(seen, old);
    assert_ne!(std::fs::read(&path).unwrap(), old);
    let names: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names, ["grids.brdc"]);
    // A directory where the file would go: the rename fails, the directory stays as it
    // was, and the hidden file is removed.
    let blocked = dir.join("blocked");
    std::fs::create_dir(&blocked).unwrap();
    assert!(write(&blocked, &[entry(3.0)]).is_err());
    assert!(std::fs::read_dir(&blocked).unwrap().next().is_none());
    let mut names: Vec<_> =
        std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
    names.sort();
    assert_eq!(names, ["blocked", "grids.brdc"]);
}

/// Writes to one path from several threads at once each replace the file whole (the
/// re-review of P5's fixes): every read finds one write's bytes, and nothing is left beside
/// the file once they end.
#[test]
fn writes_at_once_each_replace_the_file_whole() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("written_at_once");
    let _fresh = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("grids.brdc");
    let t = build(&[[3, 3, 3], [40, 0, 0]], &[]);
    let files: Vec<Vec<u8>> = (0..8)
        .map(|i| {
            to_bytes(&[CacheEntry::new(&meta("g", &[]), &t, &Field::new(&t, i as f32)).unwrap()])
                .unwrap()
        })
        .collect();
    std::thread::scope(|s| {
        for i in 0..8_usize {
            let (t, path) = (&t, &path);
            s.spawn(move || {
                for _ in 0..25 {
                    let f = Field::new(t, i as f32);
                    write(path, &[CacheEntry::new(&meta("g", &[]), t, &f).unwrap()]).unwrap();
                }
            });
        }
    });
    let last = std::fs::read(&path).unwrap();
    assert!(files.contains(&last), "the file is none of the writes");
    let names: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names, ["grids.brdc"]);
}

#[test]
fn a_file_round_trips() {
    let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("round_trip.brdc");
    let t = build(&[[3, 3, 3]], &[]);
    let f = numbered(&t, 0.0_f64, f64::from);
    write(&path, &[CacheEntry::new(&meta("on disk", &[]), &t, &f).unwrap()]).unwrap();
    let file = brodo::cache::read(&path, u64::MAX).unwrap();
    assert_eq!(
        file.grids()[0].view::<f64>().unwrap().get([3, 3, 3]).unwrap().to_bits(),
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

/// A file of no grid is its header alone (docs/formats.md §3), and is read as such.
#[test]
fn a_file_of_no_grid_is_its_header() {
    let bytes = to_bytes(&[]).unwrap();
    assert_eq!(bytes.len(), 32);
    assert_eq!(brodo::cache::from_bytes(&bytes).unwrap().grids().len(), 0);
    let mut longer = bytes.clone();
    longer.extend([0; 32]);
    assert!(matches!(brodo::cache::from_bytes(&longer), Err(CacheError::Inconsistent("end"))));
}

/// Bounds that say nothing of a grid are refused (docs/formats.md §3.4, rule 4): an empty
/// box's corners not zero, and a box whose least corner passes its greatest.
#[test]
fn bounds_that_contradict_themselves_are_refused() {
    // Grid 0's metadata: the name "g" and the unit "mm", 8 bytes each, the voxel size and
    // the transform, 120 bytes, then the bounds kind and the six corners.
    let at = |b: &[u8]| usize::try_from(word(b, 8)).unwrap() * 8 + 34;
    let read = |b: &[u8]| brodo::cache::from_bytes(b).map(|_| ());
    let empty = Topology::new();
    let field = Field::new(&empty, 0.0_f32);
    let bytes = to_bytes(&[CacheEntry::new(&meta("g", &[]), &empty, &field).unwrap()]).unwrap();
    assert_eq!(word(&bytes, at(&bytes)), 0, "kind");
    assert!(read(&bytes).is_ok());
    assert!(matches!(
        read(&with_word(&bytes, at(&bytes) + 1, 1)),
        Err(CacheError::Inconsistent(_))
    ));
    let t = build(&[[3, 3, 3]], &[]);
    let field = Field::new(&t, 0.0_f32);
    let bytes = to_bytes(&[CacheEntry::new(&meta("g", &[]), &t, &field).unwrap()]).unwrap();
    let k = at(&bytes);
    assert_eq!(word(&bytes, k), 1, "kind");
    assert!(read(&bytes).is_ok());
    let past = word(&bytes, k + 4) + 1;
    assert!(matches!(
        read(&with_word(&bytes, k + 1, past)),
        Err(CacheError::Inconsistent("bounds"))
    ));
}

#[test]
fn each_check_refuses_its_damage() {
    let g = golden();
    let read = |b: &[u8]| brodo::cache::from_bytes(b).map(|_| ());
    assert!(matches!(read(&with_word(&g, 0, 9)), Err(CacheError::BadMagic(9))));
    assert!(matches!(read(&with_word(&g, 1, 3)), Err(CacheError::UnsupportedVersion(3))));
    assert!(matches!(read(&with_word(&g, 1, 1)), Err(CacheError::UnsupportedVersion(1))));
    // A grid's frozen buffer of layout version 1 in a file of version 2.
    let frozen = usize::try_from(word(&g, 10)).unwrap() * 8;
    assert!(matches!(
        read(&with_word(&g, frozen + 1, 1)),
        Err(CacheError::Grid { index: 0, error: FormatError::UnsupportedVersion(1) })
    ));
    assert!(matches!(read(&g[..g.len() - 32]), Err(CacheError::Truncated("grid"))));
    assert!(matches!(read(&g[..g.len() - 1]), Err(CacheError::Inconsistent("length"))));
    // Directory of grid 0 is words 8..16: metadata at, length, grid at, blocks, type.
    let meta_at = word(&g, 8);
    assert!(matches!(read(&with_word(&g, 10, meta_at)), Err(CacheError::Inconsistent("grid"))));
    assert!(matches!(
        read(&with_word(&g, 12, 5)),
        Err(CacheError::Grid { index: 0, error: FormatError::WrongValueType { .. } })
    ));
    // A type code no value type has: refused as such, not read as any type.
    assert!(matches!(
        read(&with_word(&g, 12, 7)),
        Err(CacheError::Grid { index: 0, error: FormatError::UnknownValueType(7) })
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
    let read = |b: &[u8]| brodo::cache::from_bytes(b).map(|_| ());
    let meta_at = usize::try_from(word(&g, 8)).unwrap() * 32;
    // Nothing after the last grid.
    let mut longer = g.clone();
    longer.extend_from_slice(&[0; 32]);
    assert!(matches!(read(&longer), Err(CacheError::Inconsistent("end"))));
    // The header's and the directory's unused words are zero.
    assert!(matches!(read(&with_word(&g, 3, 1)), Err(CacheError::Inconsistent("header"))));
    assert!(matches!(read(&with_word(&g, 14, 1)), Err(CacheError::Inconsistent("directory"))));
    assert!(matches!(read(&with_word(&g, 13, 2)), Err(CacheError::Inconsistent("directory"))));
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
        if let Ok(file) = brodo::cache::from_bytes(&bytes) {
            for g in file.grids() {
                let _meta = g.meta();
                if let Ok(view) = g.view::<f32>() {
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
    let (mapped, copied) = (
        {
            // SAFETY: no process changes this test's own files while they are mapped.
            unsafe { CacheFile::map(&path) }
        }
        .unwrap(),
        brodo::cache::from_bytes(&bytes).unwrap(),
    );
    assert_eq!(mapped.grids().len(), copied.grids().len());
    for i in 0..copied.grids().len() {
        assert_eq!(mapped.grids()[i].meta(), copied.grids()[i].meta());
        assert_eq!(mapped.grids()[i].bounds(), copied.grids()[i].bounds());
        assert_eq!(mapped.grids()[i].value_type(), copied.grids()[i].value_type());
    }
    let voxels = [[-1, 0, 0], [4096, 7, 9], [9, 9, 9], [0, 8200, 3], [50, 50, 50], [4100, 3, 15]];
    let (a, b) =
        (mapped.grids()[0].view::<f32>().unwrap(), copied.grids()[0].view::<f32>().unwrap());
    let (c, d) = (
        mapped.grids()[1].view::<[f32; 3]>().unwrap(),
        copied.grids()[1].view::<[f32; 3]>().unwrap(),
    );
    for v in voxels {
        assert_eq!(a.get(v).unwrap().to_bits(), b.get(v).unwrap().to_bits(), "at {v:?}");
        assert_eq!(
            c.get(v).unwrap().map(f32::to_bits),
            d.get(v).unwrap().map(f32::to_bits),
            "at {v:?}"
        );
    }
}

/// A `[f64; 3]` grid beside the committed file's two, read by copy and by mapping
/// (ADR-0029).
#[test]
#[cfg_attr(miri, ignore = "needs a real file mapping, which Miri cannot run")]
fn a_grid_of_f64_vectors_reads_copied_and_mapped() {
    let t = build(&[[-1, 0, 0], [4096, 7, 9]], &[(TileLevel::Upper, [128, 0, 0])]);
    let of = |k: u32| [f64::from(k) + 0.1, -f64::from(k) / 3.0, 1e300];
    let f = numbered(&t, [-0.5, 0.25, 1e-300], of);
    let bytes = to_bytes(&[
        CacheEntry::new(&meta("density", &[]), &t, &numbered(&t, 0.0_f32, |k| k as f32)).unwrap(),
        CacheEntry::new(&meta("velocity", &[("precision", "double")]), &t, &f).unwrap(),
    ])
    .unwrap();
    let path = tmp("f64_vectors.brdc");
    std::fs::write(&path, &bytes).unwrap();
    let bits = |x: [f64; 3]| x.map(f64::to_bits);
    for file in [
        {
            // SAFETY: no process changes this test's own files while they are mapped.
            unsafe { CacheFile::map(&path) }
        }
        .unwrap(),
        brodo::cache::from_bytes(&bytes).unwrap(),
    ] {
        assert_eq!(file.grids()[1].value_type(), ValueType::F64x3);
        assert_eq!(file.grids()[1].meta(), &meta("velocity", &[("precision", "double")]));
        let view = file.grids()[1].view::<[f64; 3]>().unwrap();
        for v in [[-1, 0, 0], [4100, 3, 15], [130, 5, 5], [50, 50, 50], [-9, 0, 0]] {
            assert_eq!(bits(view.get(v).unwrap()), bits(f.value_at(&t, v).unwrap()), "at {v:?}");
        }
        let wrong = file.grids()[1].view::<[f32; 3]>().map(|_| ());
        assert!(
            matches!(
                wrong,
                Err(CacheError::Grid {
                    index: 1,
                    error: FormatError::WrongValueType { found: ValueType::F64x3, .. }
                })
            ),
            "{wrong:?}"
        );
    }
}

#[test]
#[cfg_attr(miri, ignore = "needs a real file mapping, which Miri cannot run")]
fn a_file_unfit_to_map_is_refused() {
    // A file of no bytes, refused before it is mapped as the other readers refuse it.
    let empty = tmp("empty.brdc");
    std::fs::write(&empty, []).unwrap();
    assert!(matches!(
        {
            // SAFETY: no process changes this test's own files while they are mapped.
            unsafe { CacheFile::map(&empty) }
        },
        Err(CacheError::Truncated("header"))
    ));
    assert!(matches!(brodo::cache::read(&empty, 1024), Err(CacheError::Truncated("header"))));
    assert!(matches!(brodo::cache::from_bytes(&[]), Err(CacheError::Truncated("header"))));
    let odd = tmp("odd.brdc");
    std::fs::write(&odd, [0_u8; 33]).unwrap();
    assert!(matches!(
        {
            // SAFETY: no process changes this test's own files while they are mapped.
            unsafe { CacheFile::map(&odd) }
        },
        Err(CacheError::Inconsistent("length"))
    ));
    let damaged = tmp("damaged.brdc");
    std::fs::write(&damaged, with_word(&golden(), 0, 9)).unwrap();
    assert!(matches!(
        {
            // SAFETY: no process changes this test's own files while they are mapped.
            unsafe { CacheFile::map(&damaged) }
        },
        Err(CacheError::BadMagic(9))
    ));
    assert!(matches!(
        {
            // SAFETY: no process changes this test's own files while they are mapped.
            unsafe { CacheFile::map(&tmp("missing.brdc")) }
        },
        Err(CacheError::Io(_))
    ));
}

/// A grid thaws into the field of the type its file states, refused if what thawing it is
/// counted to make passes the limit: each leaf its values' bytes and 1024 more, each tile
/// 256.
#[test]
fn a_grid_thaws_as_its_type_within_its_limit() {
    let t = build(&[[0, 0, 0], [8, 0, 0], [0, 64, 0]], &[(TileLevel::Lower, [128, 0, 0])]);
    let m = meta("g", &[]);
    let entries = [
        CacheEntry::new(&m, &t, &numbered(&t, 0.5_f32, |n| n as f32)).unwrap(),
        CacheEntry::new(&m, &t, &numbered(&t, [0.0_f64; 3], |n| [f64::from(n); 3])).unwrap(),
    ];
    let file = brodo::cache::from_bytes(&to_bytes(&entries).unwrap()).unwrap();
    let grids = file.grids();
    let sizes = [3 * (512 * 4 + 1024) + 256, 3 * (512 * 24 + 1024) + 256];
    for (i, need) in sizes.into_iter().enumerate() {
        assert_eq!(grids[i].thaw_size().unwrap(), need);
        assert!(matches!(
            grids[i].thaw(need - 1),
            Err(CacheError::ThawTooLarge { index, need: n, limit }) if index == i && n == need && limit == need - 1
        ));
    }
    let (t0, f0) = grids[0].thaw(sizes[0]).unwrap();
    let (t1, f1) = grids[1].thaw(u64::MAX).unwrap();
    assert_eq!((f0.value_type(), f1.value_type()), (ValueType::F32, ValueType::F64x3));
    let (AnyField::F32(f0), AnyField::F64x3(f1)) = (f0, f1) else { panic!("other types") };
    let (v0, v1) = (grids[0].view::<f32>().unwrap(), grids[1].view::<[f64; 3]>().unwrap());
    for v in [[0, 0, 0], [9, 1, 2], [0, 70, 0], [130, 0, 0], [500, 0, 0]] {
        assert_eq!(f0.value_at(&t0, v).unwrap().to_bits(), v0.get(v).unwrap().to_bits());
        assert!(f1.value_at(&t1, v).unwrap().same_bits(&v1.get(v).unwrap()));
    }
    assert_eq!(grids.len(), 2);
}

fn entries_u8(t: &Topology) -> Field<u8> {
    numbered(t, 9_u8, |n| u8::try_from(n % 251).unwrap())
}

/// A grid of each value type, written to a file by `cache::write` and read back by copy:
/// each read as its type at every voxel probed.
#[test]
fn every_value_type_is_written_and_read_back() {
    let t = build(&[[-1, 0, 0], [4096, 7, 9]], &[(TileLevel::Lower, [8, 8, 8])]);
    let k = |k: u32| f64::from(k) + 0.25;
    #[expect(clippy::cast_possible_truncation, reason = "small test values")]
    let entries = [
        CacheEntry::new(&meta("f32", &[]), &t, &numbered(&t, -1.0_f32, |n| k(n) as f32)).unwrap(),
        CacheEntry::new(&meta("f64", &[]), &t, &numbered(&t, -1.0_f64, k)).unwrap(),
        CacheEntry::new(&meta("u8", &[]), &t, &entries_u8(&t)).unwrap(),
        CacheEntry::new(&meta("u32", &[]), &t, &numbered(&t, 9_u32, |n| n * 3)).unwrap(),
        CacheEntry::new(
            &meta("f32x3", &[]),
            &t,
            &numbered(&t, [0.0_f32; 3], |n| [n as f32, 1.0, 2.0]),
        )
        .unwrap(),
        CacheEntry::new(&meta("f64x3", &[]), &t, &numbered(&t, [0.0_f64; 3], |n| [k(n), 1.0, 2.0]))
            .unwrap(),
    ];
    let path = tmp("every_type.brdc");
    // A file left by an earlier run would pass for one written now.
    let _absent = std::fs::remove_file(&path);
    write(&path, &entries).unwrap();
    assert_eq!(std::fs::read(&path).ok(), Some(to_bytes(&entries).unwrap()));
    let file = brodo::cache::read(&path, u64::MAX).unwrap();
    let codes: Vec<u32> = (0..6).map(|i| file.grids()[i].value_type().code()).collect();
    assert_eq!(codes, [1, 2, 3, 4, 5, 6]);
    let (u8s, u32s, f64s) =
        (entries_u8(&t), numbered(&t, 9_u32, |n| n * 3), numbered(&t, -1.0_f64, k));
    for v in [[-1, 0, 0], [4100, 7, 9], [9, 9, 9], [500, 0, 0]] {
        assert_eq!(
            file.grids()[2].view::<u8>().unwrap().get(v).unwrap(),
            u8s.value_at(&t, v).unwrap()
        );
        assert_eq!(
            file.grids()[3].view::<u32>().unwrap().get(v).unwrap(),
            u32s.value_at(&t, v).unwrap()
        );
        let got = file.grids()[1].view::<f64>().unwrap().get(v).unwrap();
        assert_eq!(got.to_bits(), f64s.value_at(&t, v).unwrap().to_bits(), "at {v:?}");
    }
}

/// A file longer than the reader's limit is refused by its length, unread; one exactly at
/// the limit is read (ADR-0036).
#[test]
fn a_file_beyond_the_read_limit_is_refused_unread() {
    let path = tmp("limited.brdc");
    let t = build(&[[0, 0, 0]], &[]);
    write(
        &path,
        &[CacheEntry::new(&meta("small", &[]), &t, &numbered(&t, 0.0_f32, |k| k as f32)).unwrap()],
    )
    .unwrap();
    let length = std::fs::metadata(&path).unwrap().len();
    assert!(brodo::cache::read(&path, length).is_ok());
    assert!(matches!(
        brodo::cache::read(&path, length - 1),
        Err(CacheError::FileTooLarge { length: l, limit }) if l == length && limit == length - 1
    ));
}

/// A file longer than the limit is refused by the bytes read, whatever length it reports:
/// a named pipe reports none, as `/dev/zero` does, and a file can grow after its length is
/// read (ADR-0036).
#[cfg(unix)]
#[test]
fn a_file_longer_than_the_limit_is_refused_whatever_length_it_reports() {
    use std::io::Write;
    let path = tmp("longer_than_its_length.fifo");
    std::fs::remove_file(&path).unwrap_or(());
    let made = std::process::Command::new("mkfifo").arg(&path).status().unwrap();
    assert!(made.success());
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
    let writer = {
        let path = path.clone();
        std::thread::spawn(move || {
            let mut pipe = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
            // The reader stops at its limit and closes the pipe: the rest is refused.
            pipe.write_all(&vec![0_u8; 1 << 20]).unwrap_or(());
        })
    };
    let read = brodo::cache::read(&path, 1024);
    writer.join().unwrap();
    std::fs::remove_file(&path).unwrap_or(());
    assert!(matches!(read, Err(CacheError::FileTooLarge { limit: 1024, .. })), "{read:?}");
    let zero = brodo::cache::read(std::path::Path::new("/dev/zero"), 1024);
    assert!(
        matches!(zero, Err(CacheError::FileTooLarge { length: 1025, limit: 1024 })),
        "{zero:?}"
    );
}
