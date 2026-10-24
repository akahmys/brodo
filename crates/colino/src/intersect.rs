//! Segments against triangles (ARCHITECTURE.md §5.1, ADR-0030): where a segment crosses a
//! triangle, by Woop, Benthin and Wald's watertight test (JCGT 2(1), 2013), and the point
//! of a segment nearest a triangle.
//!
//! The test moves the segment's start to the origin and shears its direction onto an axis,
//! so that each of the triangle's edges is a 2D edge function, `U`, `V` or `W`, of its two
//! corners. A segment passing exactly on an edge or a corner, a value of 0, is inside, so
//! that no segment passes between triangles sharing an edge. The paper computes in `f32`
//! and decides a 0 again in `f64`; colino computes in `f64`, and decides two products that
//! round equal by their rounding errors, each exact through a fused multiply-add (ADR-0030).
//! Both faces of a triangle are hit; a segment in a triangle's plane, or a triangle with no
//! area, is not.

use crate::distance::{Point, along as at, dot, on_triangle, squared, sub};

/// Below this, a product's rounding error may itself be rounded, so two equal products are
/// left equal: 2⁵³ times the smallest normal `f64`.
const SMALL: f64 = f64::MIN_POSITIVE * 9_007_199_254_740_992.0;

/// `a·b − c·d`, of the right sign: where the two products round equal, their exact
/// difference is that of their rounding errors.
pub(crate) fn edge(a: f64, b: f64, c: f64, d: f64) -> f64 {
    let (p, q) = (a * b, c * d);
    // Compared by bits: a 0 and a −0 differ by nothing, and their difference is a zero.
    if p.to_bits() != q.to_bits() {
        return p - q;
    }
    if p.abs() < SMALL {
        return 0.0;
    }
    a.mul_add(b, -p) - c.mul_add(d, -q)
}

/// Where the segment from `a` to `b` crosses triangle `t`: the fraction `s` in [0, 1] with
/// the crossing at `a + s (b − a)`. Nothing when it misses, lies in the triangle's plane,
/// or the triangle has no area, or `a` is `b`.
pub fn crossing(a: Point, b: Point, t: [Point; 3]) -> Option<f64> {
    let d = sub(b, a);
    let mut kz = 0;
    for k in 1..3 {
        if d[k].abs() > d[kz].abs() {
            kz = k;
        }
    }
    if d[kz] == 0.0 {
        return None;
    }
    // The paper swaps the two other axes where the direction is negative, to keep a
    // triangle's winding for culling its back; both faces are hit here, and the swap would
    // only negate U, V, W and the determinant together, so it is left out.
    let (kx, ky) = ((kz + 1) % 3, (kz + 2) % 3);
    let (sx, sy) = (d[kx] / d[kz], d[ky] / d[kz]);
    let [pa, pb, pc] = t.map(|p| sub(p, a));
    // The paper scales the third coordinate by 1/D; it is kept unscaled here, and the range
    // compared with det·D instead, so that a crossing exactly at an end is not rounded off.
    let sheared = |p: Point| [p[kx] - sx * p[kz], p[ky] - sy * p[kz], p[kz]];
    let ([ax, ay, az], [bx, by, bz], [cx, cy, cz]) = (sheared(pa), sheared(pb), sheared(pc));
    let u = edge(cx, by, cy, bx);
    let v = edge(ax, cy, ay, cx);
    let w = edge(bx, ay, by, ax);
    if (u < 0.0 || v < 0.0 || w < 0.0) && (u > 0.0 || v > 0.0 || w > 0.0) {
        return None;
    }
    let det = u + v + w;
    if det == 0.0 {
        return None;
    }
    let (along, limit) = (u * az + v * bz + w * cz, det * d[kz]);
    let (along, limit) = if limit < 0.0 { (-along, -limit) } else { (along, limit) };
    if along < 0.0 || along > limit {
        return None;
    }
    Some(along / limit)
}

/// The fraction along the segment from `a` to `b` nearest `p`, the lowest of equals.
fn nearest_fraction(p: Point, a: Point, b: Point) -> f64 {
    let ab = sub(b, a);
    let length = dot(ab, ab);
    if length <= 0.0 { 0.0 } else { (dot(sub(p, a), ab) / length).clamp(0.0, 1.0) }
}

/// The nearest points of segments `a`–`b` and `p`–`q`, as fractions `(s, u)` along each:
/// the least of the interior stationary point, if inside, and each side's clamped optimum.
fn segments(a: Point, b: Point, p: Point, q: Point) -> (f64, f64) {
    let (d1, d2, r) = (sub(b, a), sub(q, p), sub(a, p));
    let (aa, ee, bb) = (dot(d1, d1), dot(d2, d2), dot(d1, d2));
    let (c, f) = (dot(d1, r), dot(d2, r));
    let mut candidates = Vec::with_capacity(5);
    let denom = aa * ee - bb * bb;
    if denom > 0.0 {
        let (s, u) = ((bb * f - c * ee) / denom, (aa * f - bb * c) / denom);
        if (0.0..=1.0).contains(&s) && (0.0..=1.0).contains(&u) {
            candidates.push((s, u));
        }
    }
    for s in [0.0, 1.0] {
        candidates.push((s, nearest_fraction(at(a, b, s), p, q)));
    }
    for u in [0.0, 1.0] {
        candidates.push((nearest_fraction(at(p, q, u), a, b), u));
    }
    let gap = |(s, u): (f64, f64)| squared(at(a, b, s), at(p, q, u));
    candidates.into_iter().fold((0.0, 0.0), |best, c| {
        let (g, h) = (gap(c), gap(best));
        if g.total_cmp(&h).then(c.0.total_cmp(&best.0)).is_lt() { c } else { best }
    })
}

/// The fraction along the segment from `a` to `b` of its point nearest triangle `t`, and
/// their squared distance: where it crosses, the crossing at distance 0; otherwise the
/// least over its ends against the triangle and the triangle's edges against it, the lowest
/// fraction of equals.
pub fn nearest(a: Point, b: Point, t: [Point; 3]) -> (f64, f64) {
    if let Some(s) = crossing(a, b, t) {
        return (s, 0.0);
    }
    let mut best = (0.0, f64::INFINITY);
    let mut offer = |s: f64, d: f64| {
        if d.total_cmp(&best.1).then(s.total_cmp(&best.0)).is_lt() {
            best = (s, d);
        }
    };
    for s in [0.0, 1.0] {
        let p = at(a, b, s);
        offer(s, squared(p, on_triangle(p, t)));
    }
    let [x, y, z] = t;
    for (p, q) in [(x, y), (y, z), (z, x)] {
        let (s, u) = segments(a, b, p, q);
        offer(s, squared(at(a, b, s), at(p, q, u)));
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2 to the power `e`, exactly, for a normal `f64`.
    fn pow2(e: i64) -> f64 {
        f64::from_bits(u64::try_from(e + 1023).unwrap_or(0) << 52)
    }

    /// Products that round equal at three sizes: near 1, the tie is decided by the rounding
    /// errors; just below `SMALL`, where an error may itself round, it is left at 0, a hit;
    /// at `SMALL` exactly, it is decided again.
    #[test]
    fn ties_are_decided_by_rounding_errors_down_to_small() {
        let (up, down) = (1.0 + f64::EPSILON, 1.0 - f64::EPSILON);
        // (1 + ε)(1 − ε) = 1 − ε², which rounds to 1.
        assert!(edge(up, down, 1.0, 1.0) < 0.0);
        assert_eq!(SMALL.to_bits(), pow2(-969).to_bits());
        // The same tie scaled to 2^−970, below SMALL.
        let h = pow2(-485);
        assert_eq!(edge(up * h, down * h, h, h).to_bits(), 0.0_f64.to_bits());
        // A product of exactly SMALL against one 2^−1073 below it, rounded up to it.
        assert!(edge(SMALL, 1.0, up, SMALL * down) > 0.0);
    }
}
