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

use std::collections::BTreeSet;

use crate::analysis::Connectivity;
use crate::error::TreeError;
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

/// The offsets, in cells, of the other cells within k steps of a cell.
fn offsets(k: u32, conn: Connectivity) -> Vec<[i64; 3]> {
    let r = reach(k);
    let mut out = Vec::new();
    for z in -r..=r {
        for y in -r..=r {
            for x in -r..=r {
                if [x, y, z] != [0; 3] && steps([x, y, z], conn) <= i64::from(k) {
                    out.push([x, y, z]);
                }
            }
        }
    }
    out
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

/// The topology with a leaf at every background cell within k steps of a leaf or tile.
pub fn dilate(t: &Topology, conn: Connectivity, k: u32) -> Result<(Topology, Remap), TreeError> {
    let mut e = t.edit();
    for o in grown(t, conn, k)? {
        e.activate(o)?;
    }
    e.commit()
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

/// Whether a background cell, or the space beyond the `i32` range, lies within k steps.
fn near_background(t: &Topology, cell: [i64; 3], near: &[[i64; 3]]) -> bool {
    near.iter().any(|d| origin(add(cell, *d)).is_none_or(|o| t.locate(o) == Location::Background))
}

/// The topology without every leaf within k steps of a background cell; a larger tile is
/// split where its cells within reach must go.
pub fn erode(t: &Topology, conn: Connectivity, k: u32) -> Result<(Topology, Remap), TreeError> {
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
    let near = offsets(k, conn);
    let mut leaves = Vec::new();
    for n in t.leaf_numbers() {
        let o = t.leaf_origin(n)?;
        if near_background(t, index(o), &near) {
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
                    if near_background(t, cell, &near) {
                        split.extend(origin(cell));
                    }
                }
            }
        }
    }
    Ok((leaves, split))
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
}
