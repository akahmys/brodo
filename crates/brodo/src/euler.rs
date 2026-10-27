//! The Euler characteristic (ARCHITECTURE.md §4.5, ADR-0024) of the voxels whose value
//! meets a condition: χ = n₀ − n₁ + n₂ − n₃ over the cells of their closed unit cubes
//! (26-connectivity), or of the dual complex of their open cubes (6-connectivity).
//!
//! Each lattice vertex owns itself and the three edges, three faces and one cube on its
//! positive side, all among the 8 voxels around it, so χ is a sum over vertices of a value
//! of their 2×2×2 pattern. A vertex among 8 voxels of one kind adds nothing. A vertex
//! touching a leaf is summed by the leaf holding the first of its voxels, x fastest, in
//! parallel; elsewhere the voxels meeting the condition are the tiles that meet it, and
//! only the vertices on their edges can add anything.

use rayon::prelude::*;

use crate::analysis::{AnalysisError, Connectivity};
use crate::field::{Field, Value};
use crate::gather::{GATHER_EDGE, GATHER_VOXELS, gather};
use crate::manifest::Offset;
use crate::topology::{LeafNumber, Location, TileNumber, Topology};
use crate::tree::Coord;

/// The cells a vertex owns: their dimension, and the voxels around each among its 8, as
/// bits numbered x fastest with 1 on the positive side (bit `i + 2j + 4k`).
const CELLS: [(u32, u8); 8] = [
    (0, 0xff), // the vertex
    (1, 0xaa), // the edge along +x: the voxels with i = 1
    (1, 0xcc), // +y
    (1, 0xf0), // +z
    (2, 0x88), // the face in +x +y
    (2, 0xa0), // +x +z
    (2, 0xc0), // +y +z
    (3, 0x80), // the cube in +x +y +z
];

/// What a vertex adds to χ by pattern: of closed cubes, and of the dual of open ones. Above
/// `table`, which the audit's call graph would otherwise read as calling itself.
const TWENTY_SIX: [i8; 256] = table(false);
const SIX: [i8; 256] = table(true);

/// What a vertex adds to χ for each pattern of the 8 voxels around it.
const fn table(six: bool) -> [i8; 256] {
    let mut out = [0_i8; 256];
    let mut pattern = 0;
    while pattern < 256 {
        let mut sum = 0_i8;
        let mut c = 0;
        while c < CELLS.len() {
            let (dim, around) = CELLS[c];
            #[expect(clippy::cast_possible_truncation, reason = "pattern is below 256")]
            let met = pattern as u8 & around;
            // Closed cubes: a cell is in when any voxel around it is. Open cubes joined by
            // faces: the dual complex, whose cell stands for one of dimension 3 − d, is in
            // when every voxel around it is.
            let (counted, odd) =
                if six { (met == around, (3 - dim) % 2 == 1) } else { (met != 0, dim % 2 == 1) };
            if counted {
                sum += if odd { -1 } else { 1 };
            }
            c += 1;
        }
        out[pattern] = sum;
        pattern += 1;
    }
    out
}

/// The Euler characteristic of the voxels of `f` whose value meets `meets`, with the
/// given connectivity. The set must be bounded: a background that meets the condition is
/// refused.
pub fn euler<T: Value>(
    t: &Topology,
    f: &Field<T>,
    meets: impl Fn(T) -> bool + Sync,
    conn: Connectivity,
) -> Result<i64, AnalysisError> {
    f.belongs_to(t)?;
    if meets(f.background()) {
        return Err(AnalysisError::Unbounded);
    }
    let table = match conn {
        Connectivity::Six => &SIX,
        Connectivity::TwentySix => &TWENTY_SIX,
    };
    let numbers: Vec<LeafNumber> = t.leaf_numbers().collect();
    // Gathered in leaf order, then folded, so that the error is the lowest-numbered leaf's
    // whatever the thread count, as `par_update` gives.
    let leaves: Vec<Result<i64, AnalysisError>> =
        numbers.par_iter().map(|n| leaf_vertices(t, f, *n, &meets, table)).collect();
    let leaves = leaves.into_iter().collect::<Result<Vec<i64>, _>>()?;
    Ok(leaves.iter().sum::<i64>() + tile_edges(t, f, &meets, table)?)
}

/// Which of the 27 blocks around a leaf a gathered coordinate, -1 to 8, falls in.
fn block_of(c: [i32; 3]) -> usize {
    let step = |x: i32| usize::from(x >= 0) + usize::from(x > 7);
    step(c[0]) + 3 * step(c[1]) + 9 * step(c[2])
}

/// The sum over the vertices leaf `n` counts: those at gathered coordinates 0 to 8 whose
/// first voxel in a leaf, x fastest, is in this one.
fn leaf_vertices<T: Value>(
    t: &Topology,
    f: &Field<T>,
    n: LeafNumber,
    meets: &impl Fn(T) -> bool,
    table: &[i8; 256],
) -> Result<i64, AnalysisError> {
    let mut g = [f.background(); GATHER_VOXELS];
    gather(t, f, n, &mut g)?;
    let met: Vec<bool> = g.iter().map(|v| meets(*v)).collect();
    let mut is_leaf = [false; 27];
    is_leaf[block_of([0; 3])] = true;
    for (d, loc) in Offset::all().zip(t.neighbours(n)?) {
        let [x, y, z] = d.delta();
        is_leaf[block_of([x * 8, y * 8, z * 8])] = matches!(loc, Location::Leaf(_));
    }
    let centre = block_of([0; 3]);
    let mut sum = 0_i64;
    for c in 0..=8_i32 {
        for b in 0..=8_i32 {
            for a in 0..=8_i32 {
                let voxels = (0..8)
                    .map(|bit| [a - 1 + (bit & 1), b - 1 + (bit >> 1 & 1), c - 1 + (bit >> 2)]);
                let first = voxels.clone().map(block_of).find(|k| is_leaf[*k]);
                if first != Some(centre) {
                    continue;
                }
                let pattern = voxels.enumerate().fold(0_usize, |p, (bit, v)| {
                    let [x, y, z] = v.map(|q| usize::try_from(q + 1).unwrap_or(0));
                    p | usize::from(met[x + GATHER_EDGE * (y + GATHER_EDGE * z)]) << bit
                });
                sum += i64::from(table[pattern]);
            }
        }
    }
    Ok(sum)
}

/// A voxel's coordinates, or nothing beyond the `i32` range, which lies outside the tree.
fn voxel(v: [i64; 3]) -> Option<Coord> {
    Some([i32::try_from(v[0]).ok()?, i32::try_from(v[1]).ok()?, i32::try_from(v[2]).ok()?])
}

/// The sum over the vertices on the edges of the tiles that meet the condition, each once,
/// leaving out those touching a leaf, which the leaves count. The tiles are summed in
/// parallel; a vertex on several tiles' edges is summed by the first of them among its 8
/// voxels, x fastest, so that no set of vertices is kept.
fn tile_edges<T: Value>(
    t: &Topology,
    f: &Field<T>,
    meets: &(impl Fn(T) -> bool + Sync),
    table: &[i8; 256],
) -> Result<i64, AnalysisError> {
    let numbers: Vec<TileNumber> = t.tile_numbers().collect();
    let sums: Vec<Result<i64, AnalysisError>> =
        numbers.par_iter().map(|n| tile_vertices(t, f, *n, meets, table)).collect();
    sums.into_iter().sum()
}

/// Whether `p` lies on an edge of the region of edge `edge` at `o`: within it, and on its
/// boundary along two axes or three.
fn on_edge(o: [i64; 3], edge: i64, p: [i64; 3]) -> bool {
    let within = (0..3).all(|k| o[k] <= p[k] && p[k] <= o[k] + edge);
    let bounding = (0..3).filter(|k| p[*k] == o[*k] || p[*k] == o[*k] + edge).count();
    within && bounding >= 2
}

/// The vertices on the edges of the region of edge `edge` at `o`, each once: each edge
/// without its ends, then the eight corners.
fn edge_vertices(o: [i64; 3], edge: i64) -> Vec<[i64; 3]> {
    let mut out = Vec::new();
    for axis in 0..3 {
        let (u, w) = ((axis + 1) % 3, (axis + 2) % 3);
        for du in [0, edge] {
            for dw in [0, edge] {
                for along in 1..edge {
                    let mut p = o;
                    p[axis] += along;
                    p[u] += du;
                    p[w] += dw;
                    out.push(p);
                }
            }
        }
    }
    for corner in 0..8_usize {
        let at = |k: usize| if corner >> k & 1 == 1 { edge } else { 0 };
        out.push([o[0] + at(0), o[1] + at(1), o[2] + at(2)]);
    }
    out
}

/// The sum over the vertices on tile `n`'s edges that it is the first to hold, if it meets
/// the condition.
fn tile_vertices<T: Value>(
    t: &Topology,
    f: &Field<T>,
    n: TileNumber,
    meets: &impl Fn(T) -> bool,
    table: &[i8; 256],
) -> Result<i64, AnalysisError> {
    if !meets(f.tile(t, n)?) {
        return Ok(0);
    }
    let (level, origin) = t.tile(n)?;
    let edge = i64::from(level.edge());
    let o = origin.map(i64::from);
    let mut sum = 0_i64;
    'vertex: for p in edge_vertices(o, edge) {
        let mut pattern = 0_usize;
        let mut first = None;
        for bit in 0..8_usize {
            let step = |k: usize| i64::from(bit >> k & 1 == 1);
            let v = [p[0] - 1 + step(0), p[1] - 1 + step(1), p[2] - 1 + step(2)];
            let value = match voxel(v).map(|c| t.locate(c)) {
                Some(Location::Leaf(_)) => continue 'vertex,
                Some(Location::Tile(m, _)) => {
                    let value = f.tile(t, m)?;
                    if first.is_none() && meets(value) {
                        let (l, at) = t.tile(m)?;
                        if on_edge(at.map(i64::from), i64::from(l.edge()), p) {
                            first = Some(m);
                        }
                    }
                    value
                }
                Some(Location::Background) | None => f.background(),
            };
            pattern |= usize::from(meets(value)) << bit;
        }
        if first == Some(n) {
            sum += i64::from(table[pattern]);
        }
    }
    Ok(sum)
}
