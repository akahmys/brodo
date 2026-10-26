//! The topology report's third layer (ARCHITECTURE.md §5.2, ADR-0028): places thinner, or
//! gaps narrower, than two voxels, found from the mesh.
//!
//! Points are sampled on every triangle with area, the centroids of a k-fold split of it,
//! k the least that makes its longest edge's parts no longer than h/2. From each, the
//! nearest other triangle is found along the triangle's normal, both ways, by the watertight
//! test through the tree (`Bvh::first_crossing`): a single ray each way, from just off the triangle's plane,
//! the shape diameter function of Shapira et al. without its cone, as Cabiddu and Attene
//! describe it (Computers & Graphics 66, 2017, §2.2). A side whose midpoint is inside, by
//! the conversion's sign, is a wall that thick, otherwise a gap that wide; below 2h, the
//! point is thin. Thin points of one kind within 2h of each other are one place, so that a
//! wall's two sides are one. Points are measured in parallel, each on its own, and joined
//! in triangle order, so the places do not depend on the thread count.

use std::collections::BTreeMap;

use rayon::prelude::*;

use crate::bvh::Bvh;
use crate::convert::Inside;
use crate::distance::{Point, cross, dot, sub};
use crate::report::{Thin, ThinKind};

/// A thin point: its kind, where it lies, and its thickness or width.
#[derive(Debug, Clone, Copy)]
struct Found {
    kind: ThinKind,
    at: Point,
    measure: f64,
}

fn add(a: Point, b: Point) -> Point {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: Point, s: f64) -> Point {
    a.map(|x| x * s)
}

fn length(a: Point) -> f64 {
    dot(a, a).sqrt()
}

/// The points sampled on triangle `t`, at most `step` apart along its longest edge.
fn samples(t: [Point; 3], step: f64) -> Vec<Point> {
    let [a, b, c] = t;
    let longest = length(sub(b, a)).max(length(sub(c, b))).max(length(sub(a, c)));
    #[expect(clippy::cast_possible_truncation, reason = "a count of parts, at least 1")]
    #[expect(clippy::cast_sign_loss, reason = "a count of parts, at least 1")]
    let k = (longest / step).ceil().max(1.0) as u32;
    let (u, v) = (sub(b, a), sub(c, a));
    let mut out = Vec::new();
    for i in 0..k {
        for j in 0..k - i {
            let s = (f64::from(i) + 1.0 / 3.0) / f64::from(k);
            let r = (f64::from(j) + 1.0 / 3.0) / f64::from(k);
            out.push(add(a, add(scale(u, s), scale(v, r))));
        }
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
    let Some(root) = bvh.nodes.first().filter(|n| n.count > 0) else { return Vec::new() };
    let reach = 2.0 * length(sub(root.bounds.hi, root.bounds.lo)) + 2.0 * h;
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
