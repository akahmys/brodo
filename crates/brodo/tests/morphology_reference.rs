//! Reference tier (TESTING.md §1): dilating and eroding a topology (ADR-0025) gives, cell by
//! cell, what a breadth-first search over a dense grid of voxels says: a background cell
//! becomes a leaf when one of its voxels is within k steps of an active voxel, and an
//! active cell goes when one of its voxels is within k steps of a background voxel. The
//! search knows nothing of cells' distances; the dense box is the background beyond.

use std::collections::VecDeque;

use brodo::analysis::Connectivity;
use brodo::field::{Field, uniform};
use brodo::morphology::{dilate, erode};
use brodo::topology::{Location, Topology};
use brodo::tree::{Coord, TileLevel};
use proptest::prelude::*;

/// A dense box of voxels from `lo` to `lo + side - 1` on each axis.
struct Dense {
    lo: i32,
    side: usize,
}

impl Dense {
    fn index(&self, v: Coord) -> Option<usize> {
        let [x, y, z] = v.map(|c| usize::try_from(c - self.lo).ok().filter(|c| *c < self.side));
        Some(x? + self.side * (y? + self.side * z?))
    }

    fn voxel(&self, i: usize) -> Coord {
        let s = self.side;
        [i % s, i / s % s, i / (s * s)].map(|c| i32::try_from(c).unwrap() + self.lo)
    }

    /// Steps from each voxel to the nearest voxel `seed` holds, capped at `cap`.
    fn distances(&self, seed: &[bool], conn: Connectivity, cap: u8) -> Vec<u8> {
        let mut d = vec![cap; seed.len()];
        let mut queue = VecDeque::new();
        for (i, s) in seed.iter().enumerate() {
            if *s {
                d[i] = 0;
                queue.push_back(i);
            }
        }
        let offsets: Vec<[i32; 3]> = (-1..=1)
            .flat_map(|z| (-1..=1).flat_map(move |y| (-1..=1).map(move |x| [x, y, z])))
            .filter(|o: &[i32; 3]| {
                let n: i32 = o.iter().map(|c| c.abs()).sum();
                n > 0 && (conn == Connectivity::TwentySix || n == 1)
            })
            .collect();
        while let Some(i) = queue.pop_front() {
            if d[i] + 1 >= cap {
                continue;
            }
            let v = self.voxel(i);
            for o in &offsets {
                if let Some(j) = self.index([v[0] + o[0], v[1] + o[1], v[2] + o[2]])
                    && d[j] > d[i] + 1
                {
                    d[j] = d[i] + 1;
                    queue.push_back(j);
                }
            }
        }
        d
    }

    fn cells(&self) -> impl Iterator<Item = Coord> + '_ {
        let n = i32::try_from(self.side).unwrap() / 8;
        (0..n * n * n).map(move |i| [i % n, i / n % n, i / (n * n)].map(|c| self.lo + 8 * c))
    }

    fn cell_voxels(cell: Coord) -> impl Iterator<Item = Coord> {
        (0..512).map(move |i| [cell[0] + i % 8, cell[1] + i / 8 % 8, cell[2] + i / 64])
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Kind {
    Leaf,
    Tile,
    Background,
}

fn kind(t: &Topology, v: Coord) -> Kind {
    match t.locate(v) {
        Location::Leaf(_) => Kind::Leaf,
        Location::Tile(..) => Kind::Tile,
        Location::Background => Kind::Background,
    }
}

/// Checks every cell of the box after dilating or eroding `t` by k.
fn check(
    t: &Topology,
    d: &Dense,
    conn: Connectivity,
    k: u32,
    dilating: bool,
) -> Result<(), String> {
    let (after, remap) =
        if dilating { dilate(t, conn, k) } else { erode(t, conn, k) }.map_err(|e| e.to_string())?;
    // A cell is wholly a leaf, part of a tile, or the background: one lookup fills it.
    let mut active = vec![false; d.side.pow(3)];
    for cell in d.cells() {
        if kind(t, cell) != Kind::Background {
            for v in Dense::cell_voxels(cell) {
                active[d.index(v).unwrap()] = true;
            }
        }
    }
    let cap = u8::try_from(k + 2).unwrap();
    let seed: Vec<bool> =
        if dilating { active.clone() } else { active.iter().map(|a| !a).collect() };
    let dist = d.distances(&seed, conn, cap);
    let reached = |cell: Coord| {
        Dense::cell_voxels(cell).any(|v| dist[d.index(v).unwrap()] <= u8::try_from(k).unwrap())
    };
    for cell in d.cells() {
        let before = kind(t, cell);
        let expected = match (dilating, before) {
            (true, Kind::Background) if reached(cell) => Kind::Leaf,
            (false, Kind::Leaf | Kind::Tile) if reached(cell) => Kind::Background,
            (_, k) => k,
        };
        let got = kind(&after, cell);
        if got != expected {
            return Err(format!("cell {cell:?}: {got:?}, expected {expected:?} (was {before:?})"));
        }
    }
    // Values follow: a kept leaf keeps its values, a new one starts from the background.
    let mut f = Field::new(t, -1.0_f32);
    for n in t.leaf_numbers().collect::<Vec<_>>() {
        f.leaf_mut(t, n).unwrap().fill(2.0);
    }
    let g = f.remap(&remap, &uniform::<f32>).map_err(|e| e.to_string())?;
    for cell in d.cells() {
        let want: f32 = match (kind(t, cell), kind(&after, cell)) {
            (Kind::Leaf, Kind::Leaf) => 2.0,
            (_, Kind::Background) | (Kind::Background, _) => -1.0,
            (Kind::Tile, _) | (Kind::Leaf, Kind::Tile) => -1.0,
        };
        let got = g.value_at(&after, cell).map_err(|e| e.to_string())?;
        if got.to_bits() != want.to_bits() {
            return Err(format!("value at {cell:?}: {got}, expected {want}"));
        }
    }
    Ok(())
}

fn coord() -> impl Strategy<Value = Coord> {
    [-20..20_i32, -20..20, -20..20]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn cells_change_as_a_dense_search_says(
        leaves in prop::collection::vec(coord(), 1..20),
        tiles in prop::collection::vec(coord(), 0..6),
        k in 0..10_u32,
        six in any::<bool>(),
        dilating in any::<bool>(),
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
        let conn = if six { Connectivity::Six } else { Connectivity::TwentySix };
        let d = Dense { lo: -40, side: 80 };
        prop_assert_eq!(check(&t, &d, conn, k, dilating), Ok(()));
    }
}

/// An upper tile, with leaves beside it and one apart: its cells near the background go,
/// split from it, and the rest stays tile.
#[test]
fn an_upper_tile_is_split_where_it_must_go() {
    let empty = Topology::new();
    let mut e = empty.edit();
    e.add_tile(TileLevel::Upper, [0, 0, 0]).unwrap();
    for v in [[128, 0, 0], [128, 8, 0], [-16, 64, 64], [40, 136, 40]] {
        e.activate(v).unwrap();
    }
    let (t, _) = e.commit().unwrap();
    let d = Dense { lo: -16, side: 160 };
    for conn in [Connectivity::Six, Connectivity::TwentySix] {
        let cases = match conn {
            Connectivity::Six => [(3, false), (2, true)],
            Connectivity::TwentySix => [(1, false), (9, false)],
        };
        for (k, dilating) in cases {
            assert_eq!(
                check(&t, &d, conn, k, dilating),
                Ok(()),
                "{conn:?} k={k} dilate={dilating}"
            );
        }
    }
}
