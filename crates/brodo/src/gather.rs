//! The gather (ARCHITECTURE.md §4.4, ADR-0015): a leaf and one ghost layer from its 26
//! neighbours, copied into a dense 10³ array. A neighbour that is not a leaf gives its
//! tile's value or the background. The width is one layer; B-002 is open.

use crate::error::FieldError;
use crate::field::{Field, LEAF_VOXELS, Value, local_index};
use crate::manifest::Offset;
use crate::topology::{LeafNumber, Location, Topology};

/// Voxels along a gathered array's edge: a leaf's 8 and one ghost voxel each side.
pub const GATHER_EDGE: usize = 10;
/// Voxels in a gathered array.
pub const GATHER_VOXELS: usize = GATHER_EDGE * GATHER_EDGE * GATHER_EDGE;

/// Where one of the 27 blocks of a gather takes its values.
#[derive(Clone, Copy)]
enum Source<'a, T> {
    Leaf(&'a [T; LEAF_VOXELS]),
    Value(T),
}

/// The place of a block among the 27 around a leaf, from its step in leaves, x fastest.
fn block(d: [i32; 3]) -> usize {
    let c = (d[0] + 1) + 3 * (d[1] + 1) + 9 * (d[2] + 1);
    // Lossless: each step is -1, 0 or 1, so the place is below 27.
    c.cast_unsigned() as usize
}

/// The sources of the leaf `n` and its 26 neighbours, by block.
fn sources<'a, T: Value>(
    t: &Topology,
    f: &'a Field<T>,
    n: LeafNumber,
) -> Result<[Source<'a, T>; 27], FieldError> {
    let mut out = [Source::Value(f.background()); 27];
    out[block([0, 0, 0])] = Source::Leaf(f.leaf(t, n)?);
    let row = t.neighbours(n).map_err(|_| FieldError::UnknownLeaf(n.get()))?;
    for d in Offset::all() {
        out[block(d.delta())] = match row[d.index()] {
            Location::Leaf(m) => Source::Leaf(f.leaf(t, m)?),
            Location::Tile(tile, _) => Source::Value(f.tile(t, tile)?),
            Location::Background => Source::Value(f.background()),
        };
    }
    Ok(out)
}

/// Copies leaf `n` of `f` and one layer around it into `out`, x fastest: the element for
/// the voxel at (x, y, z) from the leaf's origin, each from -1 to 8, is at
/// (x + 1) + 10 (y + 1) + 100 (z + 1).
pub fn gather<T: Value>(
    t: &Topology,
    f: &Field<T>,
    n: LeafNumber,
    out: &mut [T; GATHER_VOXELS],
) -> Result<(), FieldError> {
    let sources = sources(t, f, n)?;
    let cells =
        (-1..=8_i32).flat_map(|z| (-1..=8).flat_map(move |y| (-1..=8).map(move |x| [x, y, z])));
    for (o, g) in out.iter_mut().zip(cells) {
        *o = match sources[block(g.map(|c| c.div_euclid(8)))] {
            Source::Leaf(values) => values[local_index(g)],
            Source::Value(v) => v,
        };
    }
    Ok(())
}
