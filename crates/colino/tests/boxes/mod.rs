//! Axis-aligned boxes the tests convert, each of 12 triangles.

use colino::distance::Point;

/// A box from `lo` to `hi`, facing out, or in when `inward`: its 8 corners, corner i at
/// `hi` on the axes whose bit of i is set, and two triangles a face.
pub fn boxed(lo: Point, hi: Point, inward: bool) -> (Vec<Point>, Vec<[u32; 3]>) {
    let vertices =
        (0..8).map(|i| [0, 1, 2].map(|k| if i >> k & 1 == 1 { hi[k] } else { lo[k] })).collect();
    let quads: [[u32; 4]; 6] =
        [[0, 4, 6, 2], [1, 3, 7, 5], [0, 1, 5, 4], [2, 6, 7, 3], [0, 2, 3, 1], [4, 5, 7, 6]];
    let mut triangles = Vec::new();
    for [p, q, r, t] in quads {
        if inward {
            triangles.extend([[p, r, q], [p, t, r]]);
        } else {
            triangles.extend([[p, q, r], [p, r, t]]);
        }
    }
    (vertices, triangles)
}
