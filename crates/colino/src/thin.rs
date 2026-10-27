//! The topology report's third layer (ARCHITECTURE.md §5.2, ADR-0028): places thinner, or
//! gaps narrower, than two voxels, found from the mesh.
//!
//! Points are sampled on every triangle with area, about h/2 apart: at every h/2 along its
//! longest edge, the middles of its section's parts no longer than h/2. A sliver has points
//! along its length only, and turning a triangle over samples the same points. From each,
//! the nearest other triangle within 2h is found along the triangle's normal, both ways, by
//! the watertight test through the tree (`Bvh::first_crossing`): a single ray each way, from
//! just off the triangle's plane, the shape diameter function of Shapira et al. without its
//! cone, as Cabiddu and Attene describe it (Computers & Graphics 66, 2017, §2.2). A side whose midpoint is inside, by
//! the conversion's sign, is a wall that thick, otherwise a gap that wide; below 2h, the
//! point is thin. Thin points of one kind within 2h of each other are one place, so that a
//! wall's two sides are one. Points are measured in parallel, each on its own, and joined
//! in triangle order, so the places do not depend on the thread count.
//!
//! Each place is then checked: in its box, grown by 2h and aligned to the conversion's
//! voxels, the solid at h, the conversion's voxels with a negative value, and at h/2, the
//! voxel centres the conversion's sign puts inside, are compared by χ, by the solid's
//! components under 26-connectivity and by the rest's under 6, the space beyond the box
//! counted with it (ADR-0028). A box holding more than 2²⁶ voxels at h/2 is not checked.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use rayon::prelude::*;

use crate::bvh::Bvh;
use crate::convert::Inside;
use crate::distance::{Point, add, cross, dot, length, scale, sub};
use brodo::analysis::{AnalysisError, Connectivity, label};
use brodo::error::FieldError;
use brodo::euler::euler;
use brodo::field::Field;
use brodo::parallel::par_update;
use brodo::topology::Topology;
use brodo::tree::Coord;

use crate::convert::{Converted, Lattice};
use crate::report::{Check, Invariants, Thin, ThinKind};

/// A thin point: its kind, where it lies, and its thickness or width.
#[derive(Debug, Clone, Copy)]
struct Found {
    kind: ThinKind,
    at: Point,
    measure: f64,
}

/// Corners in their coordinates' order, x first.
fn order(u: Point, v: Point) -> Ordering {
    u.iter().zip(&v).map(|(x, y)| x.total_cmp(y)).find(|o| o.is_ne()).unwrap_or(Ordering::Equal)
}

/// The points sampled on triangle `t`, about `step` apart: across its longest edge, at
/// every `step` along it from its lesser corner, the triangle's section split into equal
/// parts no longer than `step`, each part's middle. A sliver has a point a section, the
/// section's middle, so points along its length only; every point lies inside its section,
/// off the edges; and the longest of equal edges, and where the sections start, are chosen
/// by the corners' coordinates, so that turning the triangle over samples the same points.
fn samples(t: [Point; 3], step: f64) -> Vec<Point> {
    let [p, q, r] = t;
    // Each edge from its lesser corner, with the corner opposite; the longest, the least
    // corners of equals.
    let edge =
        |u: Point, v: Point, w: Point| if order(v, u).is_lt() { (v, u, w) } else { (u, v, w) };
    let edges = [edge(p, q, r), edge(q, r, p), edge(r, p, q)];
    let size = |e: &(Point, Point, Point)| length(sub(e.1, e.0));
    let rank = |m: &(Point, Point, Point), e: &(Point, Point, Point)| {
        size(e).total_cmp(&size(m)).then(order(m.0, e.0)).then(order(m.1, e.1))
    };
    let (a, b, c) =
        edges.iter().copied().fold(edges[0], |m, e| if rank(&m, &e).is_gt() { e } else { m });
    let long = length(sub(b, a));
    let along = scale(sub(b, a), 1.0 / long);
    let cx = dot(sub(c, a), along);
    let across = sub(sub(c, a), scale(along, cx));
    let cy = length(across);
    let mut out = Vec::new();
    if cy > 0.0 && long > 0.0 {
        let up = scale(across, 1.0 / cy);
        // The section's height at x: rising to c above cx, falling to b. The longest edge
        // puts cx within it.
        let height = |x: f64| {
            if x <= cx { cy * x / cx.max(f64::MIN_POSITIVE) } else { cy * (long - x) / (long - cx) }
        };
        #[expect(clippy::cast_possible_truncation, reason = "a count of sections, at least 1")]
        #[expect(clippy::cast_sign_loss, reason = "a count of sections, at least 1")]
        let sections = (long / step).ceil().max(1.0) as u32;
        for i in 0..sections {
            let x = (f64::from(i) + 0.5) * long / f64::from(sections);
            // Strictly within (0, long), x has a positive height: the section holds a point.
            let w = height(x);
            #[expect(clippy::cast_possible_truncation, reason = "a count of parts, at least 1")]
            #[expect(clippy::cast_sign_loss, reason = "a count of parts, at least 1")]
            let parts = (w / step).ceil().max(1.0) as u32;
            for j in 0..parts {
                let y = (f64::from(j) + 0.5) * w / f64::from(parts);
                out.push(add(a, add(scale(along, x), scale(up, y))));
            }
        }
    }
    if out.is_empty() {
        out.push([0, 1, 2].map(|k| (p[k] + q[k] + r[k]) / 3.0));
    }
    out
}

/// The distance from `x` along `d` to the nearest triangle, within `reach`. The ray starts
/// off the plane of `x`'s own triangle, so that it does not cross it nor the triangles in
/// its plane: by 2⁻⁴⁰ of the largest coordinate the crossing is worked with, some 4096
/// times its rounding, which neither vanishes far from the origin nor grows with the mesh's
/// size alone.
fn ray(bvh: &Bvh, x: Point, d: Point, reach: f64) -> Option<f64> {
    let largest = x.iter().fold(reach, |m, c| m.max(c.abs() + reach));
    let start = largest * (-40.0_f64).exp2();
    let (a, b) = (add(x, scale(d, start)), add(x, scale(d, reach)));
    bvh.first_crossing(a, b).map(|s| start + s * (reach - start))
}

/// The thin points sampled on triangle `own`.
fn measure(bvh: &Bvh, inside: &Inside<'_>, own: u32, h: f64, reach: f64) -> Vec<Found> {
    let Some(t) = bvh.corners(own) else { return Vec::new() };
    let [a, b, c] = t;
    let n = cross(sub(b, a), sub(c, a));
    let norm = length(n);
    if norm == 0.0 {
        return Vec::new();
    }
    let n = scale(n, 1.0 / norm);
    let mut out = Vec::new();
    for x in samples(t, h / 2.0) {
        for side in [1.0, -1.0] {
            let d = scale(n, side);
            let Some(measure) = ray(bvh, x, d, reach) else { continue };
            if measure >= 2.0 * h {
                continue;
            }
            let wall = inside.at(add(x, scale(d, measure / 2.0)));
            let kind = if wall { ThinKind::Wall } else { ThinKind::Gap };
            out.push(Found { kind, at: x, measure });
        }
    }
    out
}

/// The root of `x`, halving the path.
fn find(parent: &mut [usize], mut x: usize) -> usize {
    loop {
        let p = parent.get(x).copied().unwrap_or(x);
        if p == x {
            return x;
        }
        let grand = parent.get(p).copied().unwrap_or(p);
        if let Some(slot) = parent.get_mut(x) {
            *slot = grand;
        }
        x = grand;
    }
}

/// The places of thin points of one kind: points within `reach` joined, the smaller root
/// winning, each place in the order of its first point.
fn places(points: &[Found], kind: ThinKind, reach: f64) -> Vec<Thin> {
    let mine: Vec<&Found> = points.iter().filter(|p| p.kind == kind).collect();
    #[expect(clippy::cast_possible_truncation, reason = "a cell of the world's box at 2h")]
    let cell = |p: Point| p.map(|c| (c / reach).floor() as i64);
    let mut cells: BTreeMap<[i64; 3], Vec<usize>> = BTreeMap::new();
    let mut parent: Vec<usize> = (0..mine.len()).collect();
    for (j, p) in mine.iter().enumerate() {
        let at = cell(p.at);
        for i in 0..27_i64 {
            let near = [at[0] + i % 3 - 1, at[1] + i / 3 % 3 - 1, at[2] + i / 9 - 1];
            for &k in cells.get(&near).map_or(&[][..], Vec::as_slice) {
                let q = mine.get(k).map_or(p.at, |q| q.at);
                if length(sub(p.at, q)) <= reach {
                    let (a, b) = (find(&mut parent, j), find(&mut parent, k));
                    let (lo, hi) = (a.min(b), a.max(b));
                    if let Some(slot) = parent.get_mut(hi) {
                        *slot = lo;
                    }
                }
            }
        }
        cells.entry(at).or_default().push(j);
    }
    let mut by_root: BTreeMap<usize, Thin> = BTreeMap::new();
    for (j, p) in mine.iter().enumerate() {
        let root = find(&mut parent, j);
        let place = by_root.entry(root).or_insert(Thin {
            kind,
            lo: p.at,
            hi: p.at,
            least: p.measure,
            points: 0,
            check: Check::Unchecked,
        });
        place.lo = [0, 1, 2].map(|k| place.lo[k].min(p.at[k]));
        place.hi = [0, 1, 2].map(|k| place.hi[k].max(p.at[k]));
        place.least = place.least.min(p.measure);
        place.points += 1;
    }
    by_root.into_values().collect()
}

/// The thin walls, then the narrow gaps, of the mesh `bvh` holds, sides found by `inside`,
/// at voxel size `h`.
pub(crate) fn thin(bvh: &Bvh, inside: &Inside<'_>, h: f64) -> Vec<Thin> {
    // A ray need reach no farther than the thinnest place it is looking for, 2h.
    let reach = 2.0 * h;
    let found: Vec<Found> = (0..bvh.len())
        .into_par_iter()
        .map(|own| measure(bvh, inside, own, h, reach))
        .collect::<Vec<_>>()
        .into_iter()
        .flatten()
        .collect();
    // Points within 2h of each other, of one kind, are one place.
    let join = 2.0 * h;
    let mut out = places(&found, ThinKind::Wall, join);
    out.extend(places(&found, ThinKind::Gap, join));
    out
}

/// The most voxels a place's box holds at h/2 for it to be checked (ADR-0028).
const MOST_CHECKED: i64 = 1 << 26;

/// The voxels at h/2 of the box of voxels `lo` to `hi`, excluded; none beyond `i64`.
fn fine_voxels(lo: Coord, hi: Coord) -> Option<i64> {
    (0..3).try_fold(1_i64, |n, k| n.checked_mul(2 * i64::from(hi[k]) - 2 * i64::from(lo[k])))
}

/// The voxels along one axis of the box holding a place from index `lo` to `hi`, grown by
/// 2 on each side; none unless it lies within ±`i32::MAX` / 4, so that the box at h/2 does
/// too.
fn voxel_range(lo: f64, hi: f64) -> Option<(i32, i32)> {
    let limit = f64::from(i32::MAX / 4);
    let (a, b) = (lo.floor() - 2.0, hi.ceil() + 2.0);
    if !(a.abs() < limit && b.abs() < limit) {
        return None;
    }
    #[expect(clippy::cast_possible_truncation, reason = "within ±i32::MAX / 4, checked above")]
    Some((a as i32, b as i32))
}

/// The centre of voxel `v` at h/2 on the lattice of voxel size `h` with corner `origin`.
fn fine_centre(origin: Point, h: Point, v: Coord) -> Point {
    [0, 1, 2].map(|k| origin[k] + (f64::from(v[k]) + 0.5) * h[k] / 2.0)
}

/// Whether a box of voxels `lo` to `hi` is checked: at most `MOST_CHECKED` voxels at h/2.
fn checked(lo: Coord, hi: Coord) -> bool {
    fine_voxels(lo, hi).is_some_and(|n| n <= MOST_CHECKED)
}

/// The solid in voxels `lo` to `hi`, excluded, as `solid` says: a topology of the leaves
/// covering them, 1 where solid and 0 elsewhere, the background 0.
fn boxed(
    lo: Coord,
    hi: Coord,
    solid: impl Fn(Coord) -> Result<bool, FieldError> + Sync,
) -> Result<(Topology, Field<u8>), AnalysisError> {
    let empty = Topology::new();
    let mut e = empty.edit();
    let first = lo.map(|c| c.div_euclid(8) * 8);
    for z in (first[2]..hi[2]).step_by(8) {
        for y in (first[1]..hi[1]).step_by(8) {
            for x in (first[0]..hi[0]).step_by(8) {
                e.activate([x, y, z])?;
            }
        }
    }
    let (t, _) = e.commit()?;
    let mut f = Field::new(&t, 0_u8);
    let within = |v: Coord| (0..3).all(|k| lo[k] <= v[k] && v[k] < hi[k]);
    par_update(&t, &mut f, |n, values| {
        let voxels = t.leaf_voxels(n).map_err(|_| FieldError::UnknownLeaf(n.get()))?;
        for (v, x) in voxels.zip(values) {
            *x = u8::from(within(v) && solid(v)?);
        }
        Ok::<(), FieldError>(())
    })?;
    Ok((t, f))
}

fn invariants(t: &Topology, f: &Field<u8>) -> Result<Invariants, AnalysisError> {
    let solid = |x: u8| x == 1;
    Ok(Invariants {
        euler: euler(t, f, solid, Connectivity::TwentySix)?,
        solid: i64::from(label(t, f, solid, Connectivity::TwentySix)?.count()),
        complement: i64::from(label(t, f, |x| x == 0, Connectivity::Six)?.count()),
    })
}

/// The check of `place` in conversion `c`, h/2 sampled by `inside`.
pub(crate) fn check(
    c: &Converted,
    inside: &Inside<'_>,
    place: &Thin,
) -> Result<Check, AnalysisError> {
    // The report refused a transform conversion does not write before any place was found.
    let Some(Lattice { h, origin }) = c.lattice() else { return Ok(Check::Unchecked) };
    let index = |p: f64, k: usize| (p - origin[k]) / h[k];
    let (mut lo, mut hi) = ([0_i32; 3], [0_i32; 3]);
    for k in 0..3 {
        let Some(range) = voxel_range(index(place.lo[k], k), index(place.hi[k], k)) else {
            return Ok(Check::Unchecked);
        };
        (lo[k], hi[k]) = range;
    }
    if !checked(lo, hi) {
        return Ok(Check::Unchecked);
    }
    let (t, f) = boxed(lo, hi, |v| Ok(c.field.value_at(&c.topology, v)? < 0.0))?;
    let coarse = invariants(&t, &f)?;
    let (t, f) =
        boxed(lo.map(|x| 2 * x), hi.map(|x| 2 * x), |v| Ok(inside.at(fine_centre(origin, h, v))))?;
    let fine = invariants(&t, &f)?;
    Ok(if coarse == fine { Check::Same(coarse) } else { Check::Changed { coarse, fine } })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The points of a triangle whose longest edge runs along x from the origin, sampled 2
    /// apart (W2 of P5's review): sections at x = 1, 3, 5, 7, their heights 2, 10/3, 2 and
    /// 2/3, split into 1, 2, 1 and 1 parts.
    #[test]
    fn a_triangle_is_sampled_by_sections_of_its_longest_edge() {
        let t = [[0.0, 0.0, 0.0], [8.0, 0.0, 0.0], [2.0, 4.0, 0.0]];
        let want = [[1.0, 1.0], [3.0, 5.0 / 6.0], [3.0, 2.5], [5.0, 1.0], [7.0, 1.0 / 3.0]];
        let got = samples(t, 2.0);
        assert_eq!(got.len(), want.len(), "{got:?}");
        for (g, w) in got.iter().zip(want) {
            assert!((g[0] - w[0]).abs() < 1e-12 && (g[1] - w[1]).abs() < 1e-12, "{g:?} {w:?}");
            assert_eq!(g[2].to_bits(), 0.0_f64.to_bits());
        }
        // A triangle of no area has its centroid alone.
        let flat = [[0.0, 0.0, 0.0], [3.0, 3.0, 3.0], [6.0, 6.0, 6.0]];
        let line = [[1.0, 2.0, 3.0], [4.0, 2.0, 3.0], [7.0, 2.0, 3.0]];
        assert_eq!(samples(line, 2.0), [[4.0, 2.0, 3.0]]);
        assert_eq!(samples(flat, 2.0), [[3.0, 3.0, 3.0]]);
    }

    /// Only the voxels from `lo` to `hi`, excluded, are solid, though the leaves covering
    /// them hold more.
    #[test]
    fn a_box_holds_its_voxels_alone() {
        let (t, f) = boxed([1, 1, 1], [3, 3, 4], |_| Ok(true)).unwrap();
        let solid: usize = t
            .leaf_numbers()
            .map(|n| f.leaf(&t, n).unwrap().iter().filter(|v| **v == 1).count())
            .sum();
        assert_eq!(solid, 2 * 2 * 3);
    }

    /// A place's box along one axis grows by 2 voxels on each side, and lies within
    /// ±`i32::MAX` / 4; a box is checked up to `MOST_CHECKED` voxels at h/2.
    #[test]
    fn a_places_box_is_bounded_and_checked_up_to_its_limit() {
        assert_eq!(voxel_range(1.5, 3.25), Some((-1, 6)));
        let limit = f64::from(i32::MAX / 4);
        assert_eq!(voxel_range(-limit + 3.0, 0.0), Some((-(i32::MAX / 4) + 1, 2)));
        assert_eq!(voxel_range(-limit + 2.0, 0.0), None);
        assert_eq!(voxel_range(0.0, limit - 3.0), Some((-2, i32::MAX / 4 - 1)));
        assert_eq!(voxel_range(0.0, limit - 2.0), None);
        // 2^26 voxels at h/2: 256 × 512 × 512, a box of 128 × 256 × 256 voxels.
        assert!(checked([0; 3], [128, 256, 256]));
        assert!(!checked([0; 3], [128, 256, 257]));
    }

    #[test]
    fn a_voxel_at_h_over_2_is_sampled_at_its_centre() {
        let c = fine_centre([1.0, -2.0, 0.5], [0.5, 1.0, 2.0], [2, -3, 0]);
        let want: [f64; 3] = [1.625, -3.25, 1.0];
        assert_eq!(c.map(f64::to_bits), want.map(f64::to_bits));
    }

    #[test]
    fn a_box_beyond_i64_at_h_over_2_has_no_count() {
        assert_eq!(fine_voxels([0; 3], [1, 2, 3]), Some(48));
        let far = i32::MAX / 4;
        // Sides of 2^30 voxels, 2^31 at h/2: 2^93 voxels.
        assert_eq!(fine_voxels([-far; 3], [far; 3]), None);
    }
}
