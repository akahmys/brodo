//! The topology report's third layer (ARCHITECTURE.md §5.2, ADR-0028): places thinner, or
//! gaps narrower, than two voxels, found from the mesh.
//!
//! Points are sampled on every triangle with area, about h/2 apart: at every h/2 along its
//! longest edge, the middles of its section's parts no longer than h/2. A sliver has points
//! along its length only, and turning a triangle over samples the same points. From each,
//! the
//! nearest other triangle is found along the triangle's normal, both ways, by the watertight
//! test through the tree (`Bvh::first_crossing`): a single ray each way, from just off the triangle's plane,
//! the shape diameter function of Shapira et al. without its cone, as Cabiddu and Attene
//! describe it (Computers & Graphics 66, 2017, §2.2). A side whose midpoint is inside, by
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

use crate::convert::Converted;
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
            let w = height(x);
            if w.is_nan() || w <= 0.0 {
                continue;
            }
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
/// `reach`·10⁻⁹ beyond `x`, off the plane of `x`'s own triangle, so that it does not cross
/// it nor the triangles in its plane.
fn ray(bvh: &Bvh, x: Point, d: Point, reach: f64) -> Option<f64> {
    let start = 1e-9 * reach;
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
    let Some((lo, hi)) = bvh.bounds() else { return Vec::new() };
    let reach = 2.0 * length(sub(hi, lo)) + 2.0 * h;
    let found: Vec<Found> = (0..bvh.len())
        .into_par_iter()
        .map(|own| measure(bvh, inside, own, h, reach))
        .collect::<Vec<_>>()
        .into_iter()
        .flatten()
        .collect();
    let mut out = places(&found, ThinKind::Wall, 2.0 * h);
    out.extend(places(&found, ThinKind::Gap, 2.0 * h));
    out
}

/// The most voxels a place's box holds at h/2 for it to be checked (ADR-0028).
const MOST_CHECKED: i64 = 1 << 26;

/// The voxels at h/2 of the box of voxels `lo` to `hi`, excluded; none beyond `i64`.
fn fine_voxels(lo: Coord, hi: Coord) -> Option<i64> {
    (0..3).try_fold(1_i64, |n, k| n.checked_mul(2 * i64::from(hi[k]) - 2 * i64::from(lo[k])))
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
    let m = c.meta.transform;
    let (h, origin) = ([m[0][0], m[1][1], m[2][2]], [m[0][3], m[1][3], m[2][3]]);
    let limit = f64::from(i32::MAX / 4);
    let index = |p: f64, k: usize| (p - origin[k]) / h[k];
    let (mut lo, mut hi) = ([0_i32; 3], [0_i32; 3]);
    for k in 0..3 {
        let (a, b) = (index(place.lo[k], k).floor() - 2.0, index(place.hi[k], k).ceil() + 2.0);
        if !(a.abs() < limit && b.abs() < limit) {
            return Ok(Check::Unchecked);
        }
        #[expect(clippy::cast_possible_truncation, reason = "within ±i32::MAX / 4, checked above")]
        {
            (lo[k], hi[k]) = (a as i32, b as i32);
        }
    }
    if fine_voxels(lo, hi).is_none_or(|n| n > MOST_CHECKED) {
        return Ok(Check::Unchecked);
    }
    let (t, f) = boxed(lo, hi, |v| Ok(c.field.value_at(&c.topology, v)? < 0.0))?;
    let coarse = invariants(&t, &f)?;
    let centre = |v: Coord| [0, 1, 2].map(|k| origin[k] + (f64::from(v[k]) + 0.5) * h[k] / 2.0);
    let (t, f) = boxed(lo.map(|x| 2 * x), hi.map(|x| 2 * x), |v| Ok(inside.at(centre(v))))?;
    let fine = invariants(&t, &f)?;
    Ok(if coarse == fine { Check::Same(coarse) } else { Check::Changed { coarse, fine } })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_beyond_i64_at_h_over_2_has_no_count() {
        assert_eq!(fine_voxels([0; 3], [1, 2, 3]), Some(48));
        let far = i32::MAX / 4;
        // Sides of 2^30 voxels, 2^31 at h/2: 2^93 voxels.
        assert_eq!(fine_voxels([-far; 3], [far; 3]), None);
    }
}
