//! The leaf manifest (ARCHITECTURE.md §4.3, ADR-0014): what lies one leaf away from each
//! leaf in each of the 26 directions, rebuilt with its topology at each commit.

use crate::error::TreeError;
use crate::topology::{LeafNumber, Location};
use crate::tree::Coord;

/// Voxels along a leaf's edge: the step to a neighbouring leaf.
const LEAF_EDGE: i32 = 8;
/// The directions to a leaf's neighbours.
pub const DIRECTIONS: usize = 26;
/// The place of the centre among the 27 cells of a 3³ block, x fastest.
const CENTRE: u8 = 13;

/// One of the 26 directions from a leaf to a neighbour, numbered with x fastest over
/// (dx, dy, dz) in {-1, 0, 1}³, the centre left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Offset(u8);

impl Offset {
    /// Every direction, in order.
    pub fn all() -> impl Iterator<Item = Offset> {
        (0..26).map(Offset)
    }

    /// The direction's number, below 26.
    pub fn index(self) -> usize {
        usize::from(self.0)
    }

    /// The direction's place among the 27 cells of a 3³ block.
    fn cell(self) -> u8 {
        if self.0 < CENTRE { self.0 } else { self.0 + 1 }
    }

    /// The step in leaves along x, y and z, each -1, 0 or 1.
    pub fn delta(self) -> [i32; 3] {
        let c = i32::from(self.cell());
        [c % 3 - 1, (c / 3) % 3 - 1, c / 9 - 1]
    }

    /// The direction back.
    pub fn opposite(self) -> Offset {
        let c = 26 - self.cell();
        Offset(if c < CENTRE { c } else { c - 1 })
    }
}

/// What lies next to each leaf, in leaf-number order.
#[derive(Debug, Clone, Default)]
pub struct LeafManifest {
    neighbours: Vec<[Location; DIRECTIONS]>,
}

/// The origin of the leaf one step away, or nothing beyond the `i32` range.
fn step(origin: Coord, d: [i32; 3]) -> Option<Coord> {
    let mut out = origin;
    for (o, k) in out.iter_mut().zip(d) {
        *o = o.checked_add(k * LEAF_EDGE)?;
    }
    Some(out)
}

impl LeafManifest {
    /// Builds the manifest of the leaves at `origins`, in leaf-number order, locating each
    /// neighbour with `locate`, in turn, so that it may keep the path of the last. A
    /// neighbour beyond the `i32` range is the background.
    pub(crate) fn build(origins: &[Coord], mut locate: impl FnMut(Coord) -> Location) -> Self {
        let neighbours = origins
            .iter()
            .map(|o| {
                let mut row = [Location::Background; DIRECTIONS];
                for (cell, d) in row.iter_mut().zip(Offset::all()) {
                    *cell = step(*o, d.delta()).map_or(Location::Background, &mut locate);
                }
                row
            })
            .collect();
        Self { neighbours }
    }

    /// What lies next to a leaf, by direction number.
    pub fn neighbours(&self, n: LeafNumber) -> Result<&[Location; DIRECTIONS], TreeError> {
        self.neighbours.get(n.get() as usize).ok_or(TreeError::UnknownLeaf(n.get()))
    }

    /// What lies next to a leaf in one direction.
    pub fn neighbour(&self, n: LeafNumber, d: Offset) -> Result<Location, TreeError> {
        let row = self.neighbours(n)?;
        row.get(d.index()).copied().ok_or(TreeError::UnknownLeaf(n.get()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directions_cover_the_block_but_its_centre() {
        let deltas: Vec<[i32; 3]> = Offset::all().map(Offset::delta).collect();
        assert_eq!(deltas.len(), 26);
        assert_eq!(deltas[0], [-1, -1, -1]);
        assert_eq!(deltas[1], [0, -1, -1]);
        assert_eq!(deltas[12], [-1, 0, 0]);
        assert_eq!(deltas[13], [1, 0, 0]);
        assert_eq!(deltas[25], [1, 1, 1]);
        assert!(!deltas.contains(&[0, 0, 0]));
        let mut sorted = deltas.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 26);
        for d in Offset::all() {
            assert_eq!(d.opposite().delta(), d.delta().map(|c| -c));
            assert_eq!(d.opposite().opposite(), d);
        }
    }

    #[test]
    fn a_step_beyond_the_coordinate_range_is_nothing() {
        assert_eq!(step([i32::MAX - 7, 0, 0], [1, 0, 0]), None);
        assert_eq!(step([i32::MIN, 0, 0], [-1, 0, 0]), None);
        assert_eq!(step([0, 0, 0], [-1, 1, 0]), Some([-8, 8, 0]));
    }

    #[test]
    fn a_location_is_eight_bytes() {
        // The figure ADR-0014's amendment of 2026-10-17 quotes for a leaf's row.
        assert_eq!(size_of::<Location>(), 8);
        assert_eq!(size_of::<[Location; DIRECTIONS]>(), 208);
    }
}
