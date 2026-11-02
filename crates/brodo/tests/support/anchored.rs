//! Coordinates near the edges of the tree's regions, a root region's, a lower node's and a
//! leaf's, on either side of 0, where a test of the tree finds what an off-by-one breaks.

use brodo::tree::Coord;
use proptest::prelude::*;

/// A coordinate whose every axis is an edge of a region, or one past it, moved on by up to
/// `spread` − 1.
pub fn coord(spread: i32) -> impl Strategy<Value = Coord> {
    let anchors = vec![-4097, -4096, -129, -128, -9, -8, -1, 0, 7, 8, 127, 128, 4095, 4096];
    let axis = (prop::sample::select(anchors), 0..spread).prop_map(|(a, d)| a + d);
    [axis.clone(), axis.clone(), axis]
}
