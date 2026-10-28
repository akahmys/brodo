//! Dilation and erosion of a topology (ARCHITECTURE.md §4.5, ADR-0025), on whole leaves:
//! a leaf holds no per-voxel mask, so every voxel of a leaf or tile is active.
//!
//! The space is cut into leaf-sized cells, each wholly a leaf, part of a tile, or the
//! background. From a voxel of one cell to the nearest voxel of another, d cells away along
//! an axis, are (|d| − 1)·8 + 1 steps along it: the steps of a 26-connected walk are the
//! largest over the axes, of a 6-connected walk their sum. Dilating by k adds a leaf at
//! every background cell within k steps of an active one; eroding by k removes every leaf
//! within k steps of a background cell, splitting a larger tile where its cells must go.
//! Both decide on the topology given, and return the edited one with its `Remap`.
//!
//! Values are dilated and eroded on a fixed topology: each leaf voxel takes the largest,
//! or smallest, value of itself and its 6 or 26 neighbours, k times over. Tiles and the
//! background are read, never written. A NaN among them wins, so it is not lost.

use std::collections::BTreeSet;

use std::cmp::Ordering;

use crate::analysis::Connectivity;
use crate::error::{FieldError, NodeKind, TreeError};
use crate::field::{Field, Value};
use crate::gather::{GATHER_EDGE, GATHER_VOXELS, gather};
use crate::parallel::par_update;
use crate::topology::{Location, Remap, Topology};
use crate::tree::Coord;

/// Voxels along a cell's edge: a leaf's.
const CELL: i64 = 8;

/// The steps from a voxel of a cell to the nearest voxel of a cell `d` cells away.
fn steps(d: [i64; 3], conn: Connectivity) -> i64 {
    let along = d.map(|c| if c == 0 { 0 } else { (c.abs() - 1) * CELL + 1 });
    match conn {
        Connectivity::Six => along.iter().sum(),
        Connectivity::TwentySix => along.into_iter().max().unwrap_or(0),
    }
}

/// The furthest a cell within k steps can lie, in cells along one axis.
fn reach(k: u32) -> i64 {
    if k == 0 { 0 } else { (i64::from(k) - 1) / CELL + 1 }
}

/// Whether `visit` holds for an offset, in cells, of another cell within k steps of a
/// cell: the offsets are taken shell by shell outwards, each shell the cells `rho` from the
/// cell along their farthest axis, so that the first offset `visit` holds for ends it, and
/// no list of them is made.
fn any_within(k: u32, conn: Connectivity, mut visit: impl FnMut([i64; 3]) -> bool) -> bool {
    for rho in 1..=reach(k) {
        for z in -rho..=rho {
            for y in -rho..=rho {
                let on_shell = z.abs() == rho || y.abs() == rho;
                let xs: Vec<i64> = if on_shell { (-rho..=rho).collect() } else { vec![-rho, rho] };
                for x in xs {
                    if steps([x, y, z], conn) <= i64::from(k) && visit([x, y, z]) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// A cell's origin from its index, or nothing beyond the `i32` range.
fn origin(cell: [i64; 3]) -> Option<Coord> {
    let o = |c: i64| i32::try_from(c.checked_mul(CELL)?).ok();
    Some([o(cell[0])?, o(cell[1])?, o(cell[2])?])
}

fn index(v: Coord) -> [i64; 3] {
    v.map(|c| i64::from(c).div_euclid(CELL))
}

fn add(a: [i64; 3], b: [i64; 3]) -> [i64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// The cells of the active regions, by index range: a leaf's one cell, a tile's box of them.
fn regions(t: &Topology) -> Result<Vec<([i64; 3], i64)>, TreeError> {
    let mut out: Vec<([i64; 3], i64)> = t
        .leaf_numbers()
        .map(|n| t.leaf_origin(n).map(|o| (index(o), 1)))
        .collect::<Result<_, _>>()?;
    for n in t.tile_numbers() {
        let (level, o) = t.tile(n)?;
        out.push((index(o), i64::from(level.edge()) / CELL));
    }
    Ok(out)
}

/// Each cell outside the box of `side` cells at `lo` and within k steps of it, visiting only
/// the shell around it, never its inside.
fn around(lo: [i64; 3], side: i64, k: u32, conn: Connectivity, mut visit: impl FnMut([i64; 3])) {
    let r = reach(k);
    let hi = lo.map(|c| c + side);
    // The cells' distance in cells from the box along each axis, zero within it.
    let gap = |c: i64, axis: usize| {
        if c < lo[axis] {
            c - lo[axis]
        } else if c >= hi[axis] {
            c - hi[axis] + 1
        } else {
            0
        }
    };
    for z in lo[2] - r..hi[2] + r {
        for y in lo[1] - r..hi[1] + r {
            let inside = gap(y, 1) == 0 && gap(z, 2) == 0;
            let xs: Vec<i64> = if inside {
                (lo[0] - r..lo[0]).chain(hi[0]..hi[0] + r).collect()
            } else {
                (lo[0] - r..hi[0] + r).collect()
            };
            for x in xs {
                if steps([gap(x, 0), gap(y, 1), gap(z, 2)], conn) <= i64::from(k) {
                    visit([x, y, z]);
                }
            }
        }
    }
}

/// The topology with a leaf at every background cell within k steps of a leaf or tile, none
/// beyond the `i32` range. It makes as many leaves as k asks, and holds them while it does,
/// about a k³/512th of a region's shell for each region. A dilation whose new leaves a
/// `u32` could not number is refused as `TreeError::ArenaFull(NodeKind::Leaf)` before any is
/// made, the count bounded by the cells of every region's shell, overlaps counted twice, or
/// by those of the box holding the regions grown by k's reach and kept to the range,
/// whichever is fewer: one whose leaves would fit may be refused when that bound does not.
/// One that merely needs more memory than there is is not refused.
pub fn dilate(t: &Topology, conn: Connectivity, k: u32) -> Result<(Topology, Remap), TreeError> {
    within_count(&regions(t)?, k)?;
    let mut e = t.edit();
    for o in grown(t, conn, k)? {
        e.activate(o)?;
    }
    e.commit()
}

/// The cells of the box holding `regions`, its least and its greatest bound, excluded;
/// nothing for none.
fn bounds(regions: &[([i64; 3], i64)]) -> Option<([i64; 3], [i64; 3])> {
    let mut it = regions.iter();
    let (lo, side) = it.next()?;
    let mut b = (*lo, lo.map(|c| c + side));
    for (lo, side) in it {
        b.0 = [0, 1, 2].map(|k| b.0[k].min(lo[k]));
        b.1 = [0, 1, 2].map(|k| b.1[k].max(lo[k] + side));
    }
    Some(b)
}

/// Refuses, before any is made, a dilation by k whose new leaves could be more than a
/// `u32` numbers: at most the cells of every region's shell, and at most those of the box
/// holding them grown by k's reach and kept to the `i32` range.
fn within_count(regions: &[([i64; 3], i64)], k: u32) -> Result<(), TreeError> {
    let r = reach(k);
    let wide = |n: i64| u128::try_from(n).unwrap_or(0);
    let cube = |n: i64| wide(n).saturating_mul(wide(n)).saturating_mul(wide(n));
    let shells = regions.iter().fold(0_u128, |sum, (_, side)| {
        sum.saturating_add(cube(side + 2 * r).saturating_sub(cube(*side)))
    });
    let (first, last) = (i64::from(i32::MIN) / CELL, i64::from(i32::MAX) / CELL + 1);
    let grown = bounds(regions).map_or(0, |(lo, hi)| {
        (0..3).fold(1_u128, |v, k| {
            let side = (hi[k] + r).min(last) - (lo[k] - r).max(first);
            v.saturating_mul(wide(side))
        })
    });
    if shells.min(grown) > u128::from(u32::MAX) {
        return Err(TreeError::ArenaFull(NodeKind::Leaf));
    }
    Ok(())
}

/// The origins of the background cells dilating by k makes leaves.
fn grown(t: &Topology, conn: Connectivity, k: u32) -> Result<BTreeSet<Coord>, TreeError> {
    let mut cells = BTreeSet::new();
    for (lo, side) in regions(t)? {
        around(lo, side, k, conn, |c| {
            if let Some(o) = origin(c).filter(|o| t.locate(*o) == Location::Background) {
                cells.insert(o);
            }
        });
    }
    Ok(cells)
}

/// Whether a background cell, or the space beyond the `i32` range, lies within k steps;
/// the nearest are looked at first, so a cell by the background is found at once.
fn near_background(t: &Topology, cell: [i64; 3], k: u32, conn: Connectivity) -> bool {
    any_within(k, conn, |d| {
        origin(add(cell, d)).is_none_or(|o| t.locate(o) == Location::Background)
    })
}

/// The topology without every leaf within k steps of a background cell, the space beyond
/// the `i32` range counted as background; a larger tile is split where its cells within
/// reach must go.
pub fn erode(t: &Topology, conn: Connectivity, k: u32) -> Result<(Topology, Remap), TreeError> {
    // Every cell of the box holding the regions lies within its widest side's steps of the
    // background beyond it: a larger k removes no more.
    let widest =
        bounds(&regions(t)?).map_or(0, |(lo, hi)| (0..3).map(|a| hi[a] - lo[a]).max().unwrap_or(0));
    let k = u32::try_from(widest.saturating_mul(CELL)).map_or(k, |w| k.min(w));
    let (leaves, split) = removed(t, conn, k)?;
    let mut e = t.edit();
    for o in &split {
        e.split_tile(*o)?;
    }
    for o in leaves.iter().chain(&split) {
        e.deactivate(*o)?;
    }
    e.commit()
}

/// The origins of the leaves eroding by k removes, and of the tile cells it splits off and
/// removes.
fn removed(
    t: &Topology,
    conn: Connectivity,
    k: u32,
) -> Result<(Vec<Coord>, BTreeSet<Coord>), TreeError> {
    let mut leaves = Vec::new();
    for n in t.leaf_numbers() {
        let o = t.leaf_origin(n)?;
        if near_background(t, index(o), k, conn) {
            leaves.push(o);
        }
    }
    // A tile's cells that can lie within reach of the background: those within reach of
    // its faces, the inside shell of its box.
    let r = reach(k);
    let mut split = BTreeSet::new();
    for n in t.tile_numbers() {
        let (level, o) = t.tile(n)?;
        let (lo, side) = (index(o), i64::from(level.edge()) / CELL);
        let edge = |c: i64| c < r || c >= side - r;
        for z in 0..side {
            for y in 0..side {
                let xs: Vec<i64> = if edge(y) || edge(z) {
                    (0..side).collect()
                } else {
                    (0..r).chain(side - r..side).collect()
                };
                for x in xs {
                    let cell = add(lo, [x, y, z]);
                    if near_background(t, cell, k, conn) {
                        split.extend(origin(cell));
                    }
                }
            }
        }
    }
    Ok((leaves, split))
}

/// A value with an order to take the largest or smallest by: the scalar value types. A
/// vector has no such order, so `[f32; 3]` has none here (ADR-0025).
pub trait Ordered: Value + PartialOrd {}

impl Ordered for f32 {}
impl Ordered for f64 {}
impl Ordered for u8 {}
impl Ordered for u32 {}

/// The value kept of two: the larger (`keep` is `Greater`) or the smaller (`Less`), or a
/// NaN, which compares with nothing.
fn pick<T: Ordered>(a: T, b: T, keep: Ordering) -> T {
    match a.partial_cmp(&b) {
        None if a.partial_cmp(&a).is_none() => a,
        None => b,
        Some(o) if o == keep.reverse() => b,
        Some(_) => a,
    }
}

/// The places in a gathered array of a voxel's neighbours, as offsets from it.
fn neighbourhood(conn: Connectivity) -> Vec<isize> {
    let edge = isize::try_from(GATHER_EDGE).unwrap_or(10);
    let mut out = Vec::new();
    for z in -1..=1_isize {
        for y in -1..=1_isize {
            for x in -1..=1_isize {
                let n = x.abs() + y.abs() + z.abs();
                if n > 0 && (conn == Connectivity::TwentySix || n == 1) {
                    out.push(x + edge * (y + edge * z));
                }
            }
        }
    }
    out
}

fn filter<T: Ordered>(
    t: &Topology,
    f: &Field<T>,
    conn: Connectivity,
    k: u32,
    keep: Ordering,
) -> Result<Field<T>, FieldError> {
    let near = neighbourhood(conn);
    let mut current = f.clone();
    for _ in 0..k {
        let mut next = current.clone();
        par_update(t, &mut next, |n, out| {
            let mut g = [current.background(); GATHER_VOXELS];
            gather(t, &current, n, &mut g)?;
            for (i, o) in out.iter_mut().enumerate() {
                let c = (i % 8 + 1)
                    + GATHER_EDGE * (i / 8 % 8 + 1)
                    + GATHER_EDGE * GATHER_EDGE * (i / 64 + 1);
                *o = near.iter().fold(g[c], |v, d| {
                    // The gather holds one layer around the leaf, so every neighbour is in it.
                    g.get(c.wrapping_add_signed(*d)).map_or(v, |w| pick(v, *w, keep))
                });
            }
            Ok::<(), FieldError>(())
        })?;
        // A pass that changes nothing leaves every later one nothing to change: a large k
        // ends once the values settle.
        let settled = same_leaves(t, &current, &next)?;
        current = next;
        if settled {
            break;
        }
    }
    Ok(current)
}

/// Whether two fields on `t` hold the same leaf values, by bits.
fn same_leaves<T: Value>(t: &Topology, a: &Field<T>, b: &Field<T>) -> Result<bool, FieldError> {
    for n in t.leaf_numbers() {
        if !a.leaf(t, n)?.iter().zip(b.leaf(t, n)?).all(|(x, y)| x.same_bits(y)) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The field with each leaf voxel the largest of itself and its neighbours, k times over.
pub fn dilate_values<T: Ordered>(
    t: &Topology,
    f: &Field<T>,
    conn: Connectivity,
    k: u32,
) -> Result<Field<T>, FieldError> {
    filter(t, f, conn, k, Ordering::Greater)
}

/// The field with each leaf voxel the smallest of itself and its neighbours, k times over.
pub fn erode_values<T: Ordered>(
    t: &Topology,
    f: &Field<T>,
    conn: Connectivity,
    k: u32,
) -> Result<Field<T>, FieldError> {
    filter(t, f, conn, k, Ordering::Less)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A root tile is too large to commit in a test: the cells it would gain and lose are
    /// counted instead, 6-connected, by one step: the cells across its faces, and its own
    /// outer layer of cells.
    #[test]
    fn a_root_tile_grows_and_shrinks_by_its_shell() {
        let empty = Topology::new();
        let mut e = empty.edit();
        e.add_tile(crate::tree::TileLevel::Root, [0, 0, 0]).unwrap();
        let (t, _) = e.commit().unwrap();
        assert_eq!(grown(&t, Connectivity::Six, 1).unwrap().len(), 6 * 512 * 512);
        let (leaves, split) = removed(&t, Connectivity::Six, 1).unwrap();
        assert!(leaves.is_empty());
        assert_eq!(split.len(), 512 * 512 * 512 - 510 * 510 * 510);
    }

    /// The box `dilate`'s documented bound grows by k's reach holds every region.
    #[test]
    fn the_box_holds_every_region() {
        assert_eq!(bounds(&[]), None);
        assert_eq!(bounds(&[([1, 2, 3], 2), ([-4, 5, 0], 1)]), Some(([-4, 2, 0], [3, 6, 5])));
    }

    /// `dilate`'s documented bound: a dilation is refused just where it passes a `u32`'s
    /// count, one region's by its shell, a region's at either end of the range by the box
    /// kept to the range, and many regions' by the box holding them, taken when it holds
    /// exactly `u32::MAX` cells.
    #[test]
    fn a_dilation_is_refused_just_beyond_a_u32s_count() {
        let refused = |regions: &[([i64; 3], i64)], k| within_count(regions, k).is_err();
        // Side 3: reach 811 (k = 6488) gives 1625³ − 27 cells, reach 812 (k = 6489) 1627³ − 27.
        assert!(!refused(&[([0; 3], 3)], 6488));
        assert!(refused(&[([0; 3], 3)], 6489));
        // A cell at either end: the box keeps 1 + r of each side, 1625³ ≤ u32::MAX < 1626³.
        let (first, last) = (i64::from(i32::MIN) / CELL, i64::from(i32::MAX) / CELL + 1);
        for lo in [first, last - 1] {
            assert!(!refused(&[([lo; 3], 1)], 8 * 1624), "{lo}");
            assert!(refused(&[([lo; 3], 1)], 8 * 1625), "{lo}");
        }
        // Reach 100: two cells whose box is 255 × 257 × 65537 = u32::MAX cells, and copies
        // of one whose shells are more.
        let mut many = vec![([0, 0, 0], 1); 528];
        many.push(([54, 56, 65336], 1));
        assert!(!refused(&many, 800));
        many.push(([54, 56, 65337], 1));
        assert!(refused(&many, 800));
    }
}
