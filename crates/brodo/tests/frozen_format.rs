//! Format tier (TESTING.md §1): `freeze` then `GridView` gives the field's values bit for bit,
//! the same input gives the same bytes at any thread count, layout version 1 keeps the bytes
//! of a committed file (TESTING.md V6), and damaged bytes are refused, never read wrongly or
//! with a panic.

use brodo::error::FormatError;
use brodo::field::{Field, Stored, Value};
use brodo::frozen::{FrozenGrid, freeze};
use brodo::topology::{Location, Topology};
use brodo::tree::{Coord, TileLevel};
use brodo::view::{GridView, ViewLocation};
use proptest::prelude::*;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

#[derive(Debug, Clone, Copy)]
enum Op {
    Activate(Coord),
    AddTile(TileLevel, Coord),
    Split(Coord),
    Merge(Coord),
}

fn coord() -> impl Strategy<Value = Coord> {
    let anchors = vec![-4097, -4096, -129, -128, -9, -8, -1, 0, 7, 8, 127, 128, 4095, 4096];
    let axis = (prop::sample::select(anchors), 0..20_i32).prop_map(|(a, d)| a + d);
    [axis.clone(), axis.clone(), axis]
}

/// Root tiles are rarer: splitting one makes 36 862 tiles.
fn level() -> impl Strategy<Value = TileLevel> {
    prop_oneof![1 => Just(TileLevel::Root), 3 => Just(TileLevel::Upper), 3 => Just(TileLevel::Lower)]
}

fn ops() -> impl Strategy<Value = Vec<Op>> {
    let kind = prop_oneof![6 => Just(0_u8), 2 => Just(1), 1 => Just(2), 1 => Just(3)];
    let edit = (kind, level(), 0..4_usize, [0..24_i32, 0..24, 0..24]);
    (prop::collection::vec(coord(), 4), prop::collection::vec(edit, 1..40)).prop_map(
        |(pool, edits)| {
            edits
                .into_iter()
                .map(|(kind, l, i, d)| {
                    let v = [0, 1, 2].map(|k| pool[i][k] + d[k]);
                    match kind {
                        0 => Op::Activate(v),
                        1 => Op::AddTile(l, v),
                        2 => Op::Split(v),
                        _ => Op::Merge(v),
                    }
                })
                .collect()
        },
    )
}

fn build(ops: &[Op]) -> Topology {
    let empty = Topology::new();
    let mut e = empty.edit();
    for op in ops {
        // Refused edits are skipped: the tree's own tests check refusals.
        let _refused = match *op {
            Op::Activate(v) => e.activate(v).map(|_| ()),
            Op::AddTile(l, v) => e.add_tile(l, v),
            Op::Split(v) => e.split_tile(v),
            Op::Merge(v) => e.merge(TileLevel::Lower, v),
        };
    }
    e.commit().unwrap().0
}

/// Distinct values: each leaf voxel and each tile its own number.
fn field(t: &Topology) -> Field<f32> {
    let mut f = Field::new(t, -1.0);
    let mut k = 0_u32;
    for n in t.leaf_numbers() {
        for v in f.leaf_mut(t, n).unwrap() {
            *v = k as f32 + 0.5;
            k += 1;
        }
    }
    for n in t.tile_numbers() {
        f.set_tile(t, n, k as f32 + 0.25).unwrap();
        k += 1;
    }
    f
}

/// The voxels to compare: every leaf voxel; each tile's origin, far corner and centre;
/// and around each, one step out on each axis, which may be the background.
fn probes(t: &Topology) -> Vec<Coord> {
    let mut out = Vec::new();
    for n in t.leaf_numbers() {
        let o = t.leaf_origin(n).unwrap();
        out.extend((0..512_i32).map(|i| [o[0] + i % 8, o[1] + i / 8 % 8, o[2] + i / 64]));
    }
    for n in t.tile_numbers() {
        let (level, o) = t.tile(n).unwrap();
        let e = (1 << level.edge_log2()) - 1;
        out.extend([o, [o[0] + e, o[1] + e, o[2] + e], [o[0] + e / 2, o[1] + e / 2, o[2] + e / 2]]);
    }
    let around: Vec<Coord> = out
        .iter()
        .step_by(97)
        .flat_map(|v| {
            (0..3).flat_map(move |a| {
                [-1, 1].map(move |s| {
                    let mut w = *v;
                    w[a] = w[a].saturating_add(s);
                    w
                })
            })
        })
        .collect();
    out.extend(around);
    out
}

fn same<T: Stored>(
    view: &GridView<'_, T>,
    t: &Topology,
    f: &Field<T>,
    v: Coord,
) -> Result<(), TestCaseError> {
    let got = view.get(v).unwrap();
    prop_assert!(got.same_bits(&f.value_at(t, v).unwrap()), "value at {:?}", v);
    match (view.locate(v).unwrap(), t.locate(v)) {
        (ViewLocation::Leaf(a), Location::Leaf(b)) => prop_assert_eq!(a, b.get()),
        (ViewLocation::Tile(_, a), Location::Tile(_, b)) => prop_assert_eq!(a, b),
        (ViewLocation::Background, Location::Background) => {}
        (a, b) => prop_assert!(false, "{:?} against {:?} at {:?}", a, b, v),
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn a_view_reads_what_was_frozen(ops in ops()) {
        let t = build(&ops);
        let f = field(&t);
        let frozen = freeze(&t, &f).unwrap();
        let view = frozen.view::<f32>().unwrap();
        prop_assert_eq!((view.leaf_count(), view.tile_count()), (t.leaf_count(), t.tile_count()));
        for v in probes(&t) {
            same(&view, &t, &f, v)?;
        }
        let in_pool = |n| rayon::ThreadPoolBuilder::new().num_threads(n).build().unwrap().install(|| freeze(&t, &f).unwrap());
        let (one, eight) = (in_pool(1), in_pool(8));
        prop_assert_eq!(one.as_bytes(), frozen.as_bytes());
        prop_assert_eq!(eight.as_bytes(), frozen.as_bytes());
    }
}

/// A topology's leaves' origins and its tiles' levels and origins, in its own order.
fn tree(t: &Topology) -> (Vec<Coord>, Vec<(TileLevel, Coord)>) {
    (
        t.leaf_numbers().map(|n| t.leaf_origin(n).unwrap()).collect(),
        t.tile_numbers().map(|n| t.tile(n).unwrap()).collect(),
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// A view thaws into the tree and values frozen, which freeze into the same bytes, and
    /// thaws on that topology, or one of the same tree; another is refused.
    #[test]
    fn a_view_thaws_into_what_was_frozen(ops in ops(), others in ops()) {
        let t = build(&ops);
        let f = field(&t);
        let frozen = freeze(&t, &f).unwrap();
        let view = frozen.view::<f32>().unwrap();
        let (thawed, values) = view.thaw().unwrap();
        prop_assert_eq!(tree(&thawed), tree(&t));
        let again = freeze(&thawed, &values).unwrap();
        prop_assert_eq!(again.as_bytes(), frozen.as_bytes());
        let on = view.thaw_on(&t).unwrap();
        let again = freeze(&t, &on).unwrap();
        prop_assert_eq!(again.as_bytes(), frozen.as_bytes());
        let other = build(&others);
        let on_other = view.thaw_on(&other);
        if tree(&other) == tree(&t) {
            prop_assert!(on_other.is_ok());
        } else {
            prop_assert_eq!(on_other.err(), Some(FormatError::OtherTopology));
        }
        let signs = freeze(&t, &Field::new(&t, 0_u8)).unwrap();
        let signs = signs.view::<u8>().unwrap();
        prop_assert!(view.same_topology(&signs) && signs.same_topology(&view));
        let apart = freeze(&other, &Field::new(&other, 0_u8)).unwrap();
        prop_assert_eq!(view.same_topology(&apart.view::<u8>().unwrap()), tree(&other) == tree(&t));
    }
}

#[test]
fn a_topology_holding_other_leaves_or_tiles_is_refused_to_thaw_on() {
    // As many leaves and tiles, one of them elsewhere or at another level; or the view's
    // leaves and tiles and one more.
    let t = |leaves: &[Coord], tiles: &[(TileLevel, Coord)]| {
        let empty = Topology::new();
        let mut e = empty.edit();
        for v in leaves {
            e.activate(*v).unwrap();
        }
        for (level, v) in tiles {
            e.add_tile(*level, *v).unwrap();
        }
        e.commit().unwrap().0
    };
    let (upper, lower) = (TileLevel::Upper, TileLevel::Lower);
    let mine = t(&[[0; 3], [8, 0, 0]], &[(upper, [128, 0, 0])]);
    let frozen = freeze(&mine, &field(&mine)).unwrap();
    let view = frozen.view::<f32>().unwrap();
    assert!(view.thaw_on(&mine).is_ok());
    for other in [
        t(&[[0; 3], [16, 0, 0]], &[(upper, [128, 0, 0])]),
        t(&[[0; 3], [8, 0, 0]], &[(upper, [256, 0, 0])]),
        t(&[[0; 3], [8, 0, 0]], &[(lower, [128, 0, 0])]),
        t(&[[0; 3], [8, 0, 0], [16, 0, 0]], &[(upper, [128, 0, 0])]),
        t(&[[0; 3], [8, 0, 0]], &[(upper, [128, 0, 0]), (lower, [16, 0, 0])]),
    ] {
        assert_eq!(view.thaw_on(&other).err(), Some(FormatError::OtherTopology));
    }
}

#[test]
fn every_value_type_round_trips_and_a_wrong_one_is_refused() {
    let t = build(&[Op::Activate([0, 0, 0]), Op::AddTile(TileLevel::Lower, [8, 0, 0])]);
    let mut f = Field::new(&t, [1.0_f32, 2.0, 3.0]);
    let n = t.leaf_numbers().next().unwrap();
    f.leaf_mut(&t, n).unwrap()[9] = [4.0, 5.0, 6.0];
    let frozen = freeze(&t, &f).unwrap();
    let view = frozen.view::<[f32; 3]>().unwrap();
    let bits = |x: [f32; 3]| x.map(f32::to_bits);
    assert_eq!(bits(view.get([1, 1, 0]).unwrap()), bits([4.0, 5.0, 6.0]));
    assert_eq!(bits(view.get([100, 0, 0]).unwrap()), bits([1.0, 2.0, 3.0]));
    let wrong = frozen.view::<f32>().map(|_| ());
    assert_eq!(wrong, Err(FormatError::WrongValueType { expected: f32::TYPE_CODE, found: 5 }));
    let bytes = Field::new(&t, 7_u8);
    let frozen = freeze(&t, &bytes).unwrap();
    assert_eq!(frozen.view::<u8>().unwrap().get([8, 0, 0]).unwrap(), 7);
}

/// A 32-byte aligned copy of some bytes, as a file read into memory would be.
#[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Clone, Copy)]
#[repr(C, align(32))]
struct Block([u8; 32]);

fn aligned(bytes: &[u8]) -> Vec<Block> {
    let mut out = vec![Block([0; 32]); bytes.len().div_ceil(32)];
    out.as_mut_bytes()[..bytes.len()].copy_from_slice(bytes);
    out
}

/// The grid of the committed file: leaves across a root boundary, a tile at each level.
fn golden_grid() -> (Topology, Field<f32>) {
    let t = build(&[
        Op::Activate([-1, 0, 0]),
        Op::Activate([4095, 7, 9]),
        Op::Activate([4096, 7, 9]),
        Op::AddTile(TileLevel::Lower, [8, 8, 8]),
        Op::AddTile(TileLevel::Upper, [128, 0, 0]),
        Op::AddTile(TileLevel::Root, [0, 8192, 0]),
    ]);
    let f = field(&t);
    (t, f)
}

fn golden() -> FrozenGrid {
    let (t, f) = golden_grid();
    freeze(&t, &f).unwrap()
}

const GOLDEN: &[u8] = include_bytes!("data/frozen_v1_f32.bin");

/// The committed grid's values as `[f64; 3]`, every word of each nonzero, the background's
/// included, so that its six words of the header are all written.
fn golden_vectors() -> (Topology, Field<[f64; 3]>) {
    let (t, f) = golden_grid();
    let of = |x: f32| {
        let x = f64::from(x);
        [x + 0.1, -x / 3.0, x * 1e10 + 0.7]
    };
    let mut g = Field::new(&t, of(f.background()));
    for n in t.leaf_numbers() {
        for (o, x) in g.leaf_mut(&t, n).unwrap().iter_mut().zip(f.leaf(&t, n).unwrap()) {
            *o = of(*x);
        }
    }
    for n in t.tile_numbers() {
        g.set_tile(&t, n, of(f.tile(&t, n).unwrap())).unwrap();
    }
    (t, g)
}

const GOLDEN_VECTORS: &[u8] = include_bytes!("data/frozen_v1_f64x3.bin");

/// `[f64; 3]` came after layout version 1 without changing it (ADR-0029): its header holds a
/// background of six words, all of them written, and every other type's bytes are as
/// before (the test above).
#[test]
fn a_grid_of_f64_vectors_keeps_its_bytes() {
    let (t, g) = golden_vectors();
    let frozen = freeze(&t, &g).unwrap();
    assert!(frozen.as_bytes() == GOLDEN_VECTORS, "the bytes of [f64; 3] grids changed");
    let copy = aligned(GOLDEN_VECTORS);
    let view = GridView::<[f64; 3]>::new(copy.as_bytes()).unwrap();
    let bits = |x: [f64; 3]| x.map(f64::to_bits);
    assert_eq!(bits(view.background()), bits(g.background()));
    for v in
        [[-1, 0, 0], [4095, 7, 9], [4096, 7, 9], [9, 9, 9], [130, 5, 5], [0, 8200, 3], [50, 50, 50]]
    {
        assert_eq!(bits(view.get(v).unwrap()), bits(g.value_at(&t, v).unwrap()), "at {v:?}");
    }
    let wrong = GridView::<[f32; 3]>::new(copy.as_bytes()).map(|_| ());
    assert_eq!(wrong, Err(FormatError::WrongValueType { expected: 5, found: 6 }));
    // Merge rules compare by bits, every component: the last, and a zero's sign.
    assert!([1.0, 2.0, 3.0_f64].same_bits(&[1.0, 2.0, 3.0]));
    assert!(![1.0, 2.0, 3.0_f64].same_bits(&[1.0, 2.0, 4.0]));
    assert!(![0.0, 2.0, 3.0_f64].same_bits(&[-0.0, 2.0, 3.0]));
}

#[test]
fn layout_version_1_keeps_its_bytes() {
    let g = golden();
    assert_eq!(g.as_bytes().len(), GOLDEN.len(), "length");
    assert!(
        g.as_bytes() == GOLDEN,
        "the bytes of layout version 1 changed; raise the version (TESTING.md V6)"
    );
    let copy = aligned(GOLDEN);
    let view = GridView::<f32>::new(copy.as_bytes()).unwrap();
    let (t, f) = golden_grid();
    for v in
        [[-1, 0, 0], [4095, 7, 9], [4096, 7, 9], [9, 9, 9], [130, 5, 5], [0, 8200, 3], [50, 50, 50]]
    {
        assert_eq!(
            view.get(v).unwrap().to_bits(),
            f.value_at(&t, v).unwrap().to_bits(),
            "at {v:?}"
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Damaged bytes: changed words, or a cut. Either refused, or every read answers
    /// without a panic.
    #[test]
    fn damaged_bytes_are_refused_or_read_safely(
        changes in prop::collection::vec((any::<prop::sample::Index>(), any::<u32>()), 1..6),
        cut in prop::option::of(any::<prop::sample::Index>()),
        header_only in any::<bool>(),
    ) {
        let g = golden();
        let mut bytes = g.as_bytes().to_vec();
        let words = if header_only { 24 } else { bytes.len() / 4 };
        for (i, x) in changes {
            let w = i.index(words);
            bytes[w * 4..w * 4 + 4].copy_from_slice(&x.to_le_bytes());
        }
        if let Some(c) = cut {
            bytes.truncate(c.index(bytes.len()) / 32 * 32);
        }
        let copy = aligned(&bytes);
        if let Ok(view) = GridView::<f32>::new(&copy.as_bytes()[..bytes.len()]) {
            for v in [[0, 0, 0], [-1, 0, 0], [4096, 7, 9], [130, 5, 5], [9, 9, 9], [0, 8200, 3]] {
                let _either = view.get(v);
            }
            let _thawed = view.thaw();
        }
    }
}

/// Changes one header word (by its place among the header's 24 words) of the golden grid.
fn with_word(word: usize, value: u32) -> Vec<Block> {
    let mut bytes = golden().as_bytes().to_vec();
    bytes[word * 4..word * 4 + 4].copy_from_slice(&value.to_le_bytes());
    aligned(&bytes)
}

#[test]
fn each_check_refuses_its_damage() {
    let read = |b: &[u8]| GridView::<f32>::new(b).map(|_| ());
    // Header words: magic, version, type, size, counts[5], offsets[5], blocks, background.
    assert_eq!(read(with_word(0, 7).as_bytes()), Err(FormatError::BadMagic(7)));
    assert_eq!(read(with_word(1, 2).as_bytes()), Err(FormatError::UnsupportedVersion(2)));
    assert_eq!(
        read(with_word(2, 2).as_bytes()),
        Err(FormatError::WrongValueType { expected: 1, found: 2 })
    );
    // The tile values moved onto the leaf values: every length and link still fits, only
    // the sections overlap.
    let g = golden();
    let leaf_values = u32::from_le_bytes(g.as_bytes()[48..52].try_into().unwrap());
    assert_eq!(
        read(with_word(13, leaf_values).as_bytes()),
        Err(FormatError::Inconsistent("tile value"))
    );
    // One more leaf than the leaf values hold.
    let leaves = u32::from_le_bytes(g.as_bytes()[28..32].try_into().unwrap());
    assert!(read(with_word(7, leaves + 1).as_bytes()).is_err());
    // A buffer one block short, and bytes not starting on a block.
    let bytes = g.as_bytes();
    assert_eq!(
        read(&aligned(bytes).as_bytes()[..bytes.len() - 32]),
        Err(FormatError::Inconsistent("header"))
    );
    let mut shifted = vec![0_u8];
    shifted.extend_from_slice(bytes);
    let shifted = aligned(&shifted);
    assert_eq!(read(&shifted.as_bytes()[1..=bytes.len()]), Err(FormatError::Misaligned));
}

/// The committed grid with words changed: each `(word, value)` set, the rest kept.
fn with_words(changes: &[(usize, u32)]) -> Vec<Block> {
    let mut bytes = golden().as_bytes().to_vec();
    for (word, value) in changes {
        bytes[word * 4..word * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    aligned(&bytes)
}

/// Each check on the links between sections refuses its own damage, the others passing.
/// The committed grid holds four root entries, from block 3 (-4096 and 0 upper nodes, a
/// root tile at y = 8192, 4096 an upper node), and three upper nodes of 257 blocks from
/// block 7; upper node 1 has two children from lower node 1 and one tile from tile 1.
#[test]
fn each_link_check_refuses_its_damage() {
    let read = |b: &[Block]| GridView::<f32>::new(b.as_bytes()).map(|_| ());
    let word =
        |at: usize| u32::from_le_bytes(golden().as_bytes()[at * 4..at * 4 + 4].try_into().unwrap());
    let root = |i: usize, field: usize| (3 + i) * 8 + field;
    let upper = |i: usize, w: usize| (7 + 257 * i) * 8 + w;
    let lost = |what| Err(FormatError::Inconsistent(what));
    assert_eq!(read(&aligned(golden().as_bytes())), Ok(()));
    // Two root entries with one key: not sorted.
    assert_eq!(read(&with_words(&[(root(1, 0), (-4096_i32).cast_unsigned())])), lost("root"));
    // A key off a root region's corner, still in order.
    assert_eq!(read(&with_words(&[(root(0, 0), (-4095_i32).cast_unsigned())])), lost("root"));
    // An upper node's number, and a root tile's, one past the last.
    assert_eq!(read(&with_words(&[(root(0, 4), 3)])), lost("root"));
    assert_eq!(read(&with_words(&[(root(2, 4), 3)])), lost("root"));
    // Upper node 1's children run one past the lower nodes; its tiles one past the tiles.
    assert_eq!(read(&with_words(&[(upper(1, 2048), 3)])), lost("upper node"));
    assert_eq!(read(&with_words(&[(upper(1, 2049), 3)])), lost("upper node"));
    // A place both a child and a tile, the counts still fitting.
    let (w, bits) = (0..1024).map(|w| (w, word(upper(1, w)))).find(|(_, b)| *b != 0).unwrap();
    let tiles = word(upper(1, 1024 + w));
    let overlap = bits.isolate_lowest_one();
    assert_eq!(tiles & overlap, 0);
    assert_eq!(
        read(&with_words(&[(upper(1, 1024 + w), tiles | overlap), (upper(1, 2049), 0)])),
        lost("upper node")
    );
}
