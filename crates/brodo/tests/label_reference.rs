//! Reference tier (TESTING.md §1): labelling a sparse grid splits voxels as a breadth-first
//! search on a dense grid does. The dense grid covers every leaf and tile with a margin; all
//! beyond it is the background, so its border stands for the outside: when the background
//! meets the condition, every component touching the border is one, the outside's.

use std::collections::VecDeque;

use brodo::analysis::{Connectivity, Label, label};
use brodo::field::Field;
use brodo::topology::Topology;
use brodo::tree::{Coord, TileLevel};
use proptest::prelude::*;

#[path = "support/lcg.rs"]
mod lcg;

/// The dense box: voxels from LO to HI - 1 on each axis; leaves and tiles lie within MARGIN
/// of the border's inside.
const LO: i32 = -20;
const HI: i32 = 20;
const SIDE: usize = 40;

fn index(v: Coord) -> usize {
    let [x, y, z] = v.map(|c| usize::try_from(c - LO).unwrap());
    x + SIDE * (y + SIDE * z)
}

fn voxel(i: usize) -> Coord {
    [i % SIDE, i / SIDE % SIDE, i / (SIDE * SIDE)].map(|c| i32::try_from(c).unwrap() + LO)
}

/// Components of the dense grid by breadth-first search, with whether each touches the
/// border while the background meets the condition.
fn dense(meets: &[bool], conn: Connectivity, background: bool) -> (Vec<Option<usize>>, Vec<bool>) {
    let offsets: Vec<[i32; 3]> = (-1..=1_i32)
        .flat_map(|z| (-1..=1).flat_map(move |y| (-1..=1).map(move |x| [x, y, z])))
        .filter(|d: &[i32; 3]| {
            let n: i32 = d.iter().map(|c| c.abs()).sum();
            n > 0 && (conn == Connectivity::TwentySix || n == 1)
        })
        .collect();
    let mut comp = vec![None; meets.len()];
    let mut outside = Vec::new();
    for start in 0..meets.len() {
        if !meets[start] || comp[start].is_some() {
            continue;
        }
        let id = outside.len();
        let mut touches = false;
        let mut queue = VecDeque::from([start]);
        comp[start] = Some(id);
        while let Some(i) = queue.pop_front() {
            let v = voxel(i);
            for d in &offsets {
                let w = [v[0] + d[0], v[1] + d[1], v[2] + d[2]];
                if w.iter().any(|c| !(LO..HI).contains(c)) {
                    touches = true;
                    continue;
                }
                let j = index(w);
                if meets[j] && comp[j].is_none() {
                    comp[j] = Some(id);
                    queue.push_back(j);
                }
            }
        }
        outside.push(touches && background);
    }
    (comp, outside)
}

fn coord() -> impl Strategy<Value = Coord> {
    [-12..12_i32, -12..12, -12..12]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]

    #[test]
    fn labels_split_voxels_as_a_dense_search_does(
        leaves in prop::collection::vec(coord(), 1..30),
        tiles in prop::collection::vec(coord(), 0..8),
        seed in any::<u64>(),
        background in any::<bool>(),
        six in any::<bool>(),
    ) {
        let empty = Topology::new();
        let mut e = empty.edit();
        for v in &leaves {
            e.activate(*v).unwrap();
        }
        for v in &tiles {
            let _occupied = e.add_tile(TileLevel::Lower, *v);
        }
        let (t, _) = e.commit().unwrap();
        let mut f = Field::new(&t, if background { 1.0_f32 } else { -1.0 });
        let mut g = lcg::Lcg(seed);
        let mut next = || u64::from(g.next());
        for n in t.leaf_numbers().collect::<Vec<_>>() {
            for v in f.leaf_mut(&t, n).unwrap() {
                *v = if next() % 5 < 3 { 1.0 } else { -1.0 };
            }
        }
        for n in t.tile_numbers().collect::<Vec<_>>() {
            f.set_tile(&t, n, if next() % 2 == 0 { 1.0 } else { -1.0 }).unwrap();
        }
        let conn = if six { Connectivity::Six } else { Connectivity::TwentySix };
        let positive = |v: f32| v > 0.0;
        let labels = label(&t, &f, positive, conn).unwrap();
        let meets: Vec<bool> = (0..SIDE * SIDE * SIDE).map(|i| positive(f.value_at(&t, voxel(i)).unwrap())).collect();
        let (comp, outside) = dense(&meets, conn, background);
        // The dense components touching the border are one when the background meets it.
        let merged = |c: usize| if outside[c] { usize::MAX } else { c };
        let mut to_sparse = std::collections::BTreeMap::new();
        let mut to_dense = std::collections::BTreeMap::new();
        for (i, dense_comp) in comp.iter().enumerate() {
            let got = labels.label_at(&t, voxel(i)).unwrap();
            match (*dense_comp, got) {
                (None, Label::Excluded) => {}
                (Some(c), Label::Component(s)) => {
                    let c = merged(c);
                    prop_assert_eq!(*to_sparse.entry(c).or_insert(s), s, "at {:?}", voxel(i));
                    prop_assert_eq!(*to_dense.entry(s).or_insert(c), c, "at {:?}", voxel(i));
                    prop_assert_eq!(labels.reaches_outside(got), c == usize::MAX, "outside at {:?}", voxel(i));
                }
                (d, s) => prop_assert!(false, "{:?} against {:?} at {:?}", d, s, voxel(i)),
            }
        }
    }
}

/// A leaf's region of the background enclosed by 26 leaves none of whose voxels meets the
/// condition is a component of its own, apart from the outside, however far into its lower
/// node it lies.
#[test]
fn an_enclosed_region_of_the_background_is_its_own_component() {
    let empty = Topology::new();
    let mut e = empty.edit();
    for z in 1..=3 {
        for y in 1..=3 {
            for x in 1..=3 {
                if [x, y, z] != [2, 2, 2] {
                    e.activate([8 * x, 8 * y, 8 * z]).unwrap();
                }
            }
        }
    }
    let (t, _) = e.commit().unwrap();
    let mut f = Field::new(&t, 1.0_f32);
    for n in t.leaf_numbers().collect::<Vec<_>>() {
        f.leaf_mut(&t, n).unwrap().fill(-1.0);
    }
    let labels = label(&t, &f, |v: f32| v > 0.0, Connectivity::Six).unwrap();
    assert_eq!(labels.count(), 2);
    let inside = labels.label_at(&t, [16, 16, 16]).unwrap();
    assert!(matches!(inside, Label::Component(_)), "{inside:?}");
    assert!(!labels.reaches_outside(inside));
    assert_eq!(labels.label_at(&t, [23, 23, 23]).unwrap(), inside);
    let outside = labels.label_at(&t, [0, 0, 0]).unwrap();
    assert!(labels.reaches_outside(outside) && outside != inside);
    assert_eq!(labels.label_at(&t, [8, 8, 8]).unwrap(), Label::Excluded);
}
