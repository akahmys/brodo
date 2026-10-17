//! Format tier (TESTING.md §1): `freeze` then `GridView` gives the field's values bit for bit,
//! the same input gives the same bytes at any thread count, layout version 1 keeps the bytes
//! of a committed file (TESTING.md V6), and damaged bytes are refused, never read wrongly or
//! with a panic.

use brodo::error::FormatError;
use brodo::field::{Field, Value};
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

fn same<T: Value>(
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
