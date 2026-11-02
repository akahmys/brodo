//! The point of a triangle nearest a point (ARCHITECTURE.md §5.1, ADR-0027), by the
//! textbook method that tells which of the triangle's seven regions, three corners, three
//! edges and the face, the point projects to. A triangle with no area, its corners on one
//! line or one point, has no face: its nearest point is its edges'.

/// A point in space.
pub type Point = [f64; 3];

pub(crate) fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn dot(a: Point, b: Point) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn cross(a: Point, b: Point) -> Point {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

pub(crate) fn add(a: Point, b: Point) -> Point {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub(crate) fn scale(a: Point, s: f64) -> Point {
    a.map(|x| x * s)
}

pub(crate) fn length(a: Point) -> f64 {
    dot(a, a).sqrt()
}

/// `a + s (b − a)`.
pub(crate) fn along(a: Point, b: Point, s: f64) -> Point {
    [a[0] + s * (b[0] - a[0]), a[1] + s * (b[1] - a[1]), a[2] + s * (b[2] - a[2])]
}

/// Whether triangle `t` has area: the cross product of its edges from its first corner is
/// not exactly zero. One with none is never crossed, nor given a normal.
pub(crate) fn has_area(t: [Point; 3]) -> bool {
    let [a, b, c] = t;
    cross(sub(b, a), sub(c, a)).iter().any(|x| *x != 0.0)
}

/// `n` scaled to unit length, nothing for a zero or a vector not finite: divided by its
/// largest component first, so that its squared length neither underflows to 0 for a
/// triangle of edges below 10⁻⁸¹ nor overflows for one above 10⁷⁷.
pub(crate) fn unit(n: Point) -> Option<Point> {
    let largest = n.iter().fold(0.0_f64, |m, x| m.max(x.abs()));
    if largest == 0.0 || !n.iter().all(|x| x.is_finite()) {
        return None;
    }
    let m = n.map(|x| x / largest);
    Some(scale(m, 1.0 / length(m)))
}

/// The squared distance between two points.
pub fn squared(a: Point, b: Point) -> f64 {
    let d = sub(a, b);
    dot(d, d)
}

/// The point of the segment from `a` to `b` nearest `p`.
pub fn on_segment(p: Point, a: Point, b: Point) -> Point {
    along(a, b, fraction_on_segment(p, a, b))
}

/// The fraction along the segment from `a` to `b` of its point nearest `p`, in [0, 1]; 0
/// for a segment of no length.
pub(crate) fn fraction_on_segment(p: Point, a: Point, b: Point) -> f64 {
    let ab = sub(b, a);
    let length = dot(ab, ab);
    if length <= 0.0 { 0.0 } else { (dot(sub(p, a), ab) / length).clamp(0.0, 1.0) }
}

/// The mean of a triangle's corners.
pub(crate) fn centroid(t: [Point; 3]) -> Point {
    let [a, b, c] = t;
    [0, 1, 2].map(|k| (a[k] + b[k] + c[k]) / 3.0)
}

/// A triangle's area, its cross product divided by its largest component before its length
/// is taken, so that it neither underflows to 0 for a triangle of edges below 10⁻⁸¹ nor
/// overflows for one above 10⁷⁷, as `unit` does.
pub(crate) fn area(t: [Point; 3]) -> f64 {
    let [a, b, c] = t;
    let n = cross(sub(b, a), sub(c, a));
    let largest = n.iter().fold(0.0_f64, |m, x| m.max(x.abs()));
    if largest == 0.0 || !largest.is_finite() {
        return 0.0;
    }
    largest * length(n.map(|x| x / largest)) / 2.0
}

/// The point of the triangle `t` nearest `p`. Of two points as near, apart by rounding
/// alone, which is given is not promised; nor, for a triangle thinner than √ε of its size,
/// which point within its width of the nearest.
pub fn on_triangle(p: Point, t: [Point; 3]) -> Point {
    let [a, b, c] = t;
    let unscaled = [sub(b, a), sub(c, a), sub(p, a)];
    let [ab, ac, ap] = unscaled;
    let lengths = [dot(ab, ab), dot(ac, ac)];
    // A triangle whose squared edges from `a` lie within `UNSCALED`, and a point no
    // farther, is taken as it is; any other is divided by its largest component first.
    if lengths.iter().all(|l| UNSCALED.contains(l)) && dot(ap, ap) <= *UNSCALED.end() {
        return in_regions(p, t, unscaled, lengths, 1.0);
    }
    // The edges and `p` from `a` divided by their largest component: the tests of
    // `in_regions` compare products of four lengths, which would under- or overflow for a
    // triangle far from 1 in size, and ratios of them, which the division leaves as they were.
    let largest = unscaled.iter().flatten().fold(0.0_f64, |m, x| m.max(x.abs()));
    if largest == 0.0 || !largest.is_finite() {
        return a;
    }
    let [ab, ac, ap] = unscaled.map(|e| e.map(|x| x / largest));
    in_regions(p, t, [ab, ac, ap], [dot(ab, ab), dot(ac, ac)], largest)
}

/// 2⁻⁴⁰⁰ to 2⁴⁰⁰: ε times a product of two squared lengths within it, the most
/// `in_regions` takes, neither under- nor overflows, 2⁻⁸⁵² and 2⁸⁰⁰ against 2⁻¹⁰²² and
/// 2¹⁰²⁴, so a triangle of such edges needs no division.
const UNSCALED: std::ops::RangeInclusive<f64> =
    3.872_591_914_849_318e-121..=2.582_249_878_086_908_6e120;

/// The point of the triangle `[a, b, c]` nearest `p` by its regions: `ab`, `ac` and `ap`
/// are its edges and `p` from `a`, and `lengths` the edges' squared lengths, each divided
/// by `unit`. Inlined always: called apart, `on_triangle` measured about 10 % slower on
/// the slab benchmark (`cargo bench -p colino`, 2026-11-03).
#[inline(always)]
fn in_regions(
    p: Point,
    [a, b, c]: [Point; 3],
    [ab, ac, ap]: [Point; 3],
    [lab, lac]: [f64; 2],
    unit: f64,
) -> Point {
    // A normal shorter than √ε of its edges' product, the corners on a line or nearly, is
    // rounding: the nearest point is taken on the edges, off by no more than the triangle's
    // width, √ε of its size. Above it, the face's point is p's projection on the plane, off
    // by ε of |ap| over the angle's sine; the barycentric weights, ratios of |n|², would be
    // off by its square.
    let n = cross(ab, ac);
    if !(dot(n, n) > 0.0 && dot(n, n) >= f64::EPSILON * lab * lac) {
        return nearest_edge(p, a, [on_segment(p, a, b), on_segment(p, b, c), on_segment(p, c, a)]);
    }
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(ap, ab);
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return along(a, b, d1 / (d1 - d3));
    }
    let cp = sub(ap, ac);
    let (d5, d6) = (dot(ab, cp), dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return along(a, c, d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        return along(b, c, (d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    sub(p, scale(n, unit * (dot(ap, n) / dot(n, n))))
}

/// Of `first` and `others`, the point nearest `p`, the earliest of those as near.
fn nearest_edge(p: Point, first: Point, others: [Point; 3]) -> Point {
    others
        .into_iter()
        .fold(first, |best, q| if squared(p, q) < squared(p, best) { q } else { best })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The vector helpers on values worked by hand: a broken one fails here, in the first
    /// test binary, before the tests that would only run slowly on it (TESTING.md T11).
    #[test]
    fn the_vector_helpers_give_what_is_worked_by_hand() {
        let (a, b) = ([1.0, -2.0, 3.0], [4.0, 0.5, -1.0]);
        let bits = |p: Point| p.map(f64::to_bits);
        assert_eq!(bits(add(a, b)), bits([5.0, -1.5, 2.0]));
        assert_eq!(bits(sub(a, b)), bits([-3.0, -2.5, 4.0]));
        assert_eq!(bits(scale(a, 2.0)), bits([2.0, -4.0, 6.0]));
        assert_eq!(bits(cross([1.0, 0.0, 0.0], [0.0, 1.0, 0.0])), bits([0.0, 0.0, 1.0]));
        assert_eq!(dot(a, [2.0, 1.0, -1.0]).to_bits(), (-3.0_f64).to_bits());
        assert_eq!(length([3.0, 4.0, 12.0]).to_bits(), 13.0_f64.to_bits());
        assert_eq!(squared(a, b).to_bits(), (9.0_f64 + 6.25 + 16.0).to_bits());
    }

    /// A normal of a triangle of edges 10⁻⁹⁰, or 10⁸⁰, whose squared length underflows to 0,
    /// or overflows, is made of unit length all the same; a zero has none, nor a vector not
    /// finite. Whether a triangle has area is its cross product's, however small.
    #[test]
    fn a_normal_of_any_size_is_made_unit() {
        for size in [1e-90, 1e-30, 1.0, 1e30, 1e80] {
            let t = [[0.0; 3], [size, 0.0, 0.0], [0.0, 2.0 * size, 0.0]];
            assert!(has_area(t), "{size}");
            let [a, b, c] = t;
            let n = unit(cross(sub(b, a), sub(c, a))).unwrap();
            assert_eq!(n.map(f64::to_bits), [0.0, 0.0, 1.0].map(f64::to_bits), "{size}");
            let tilted = unit([size, -size, 2.0 * size]).unwrap();
            // A division, a sum of squares and a square root, each a rounding of about 1.
            assert!((length(tilted) - 1.0).abs() <= 4.0 * f64::EPSILON, "{size}: {tilted:?}");
        }
        assert_eq!(unit([0.0; 3]), None);
        assert_eq!(unit([f64::INFINITY, 0.0, 0.0]), None);
        assert_eq!(unit([f64::NAN, 1.0, 0.0]), None);
        assert!(!has_area([[1.0, 2.0, 3.0], [2.0, 4.0, 6.0], [3.0, 6.0, 9.0]]));
    }

    /// A right triangle of legs `s` and `2s` has area `s²` exactly, at sizes whose cross
    /// product would underflow or overflow if squared unscaled; a tilted one √2/2 of its
    /// legs' product; one with none, 0.
    #[test]
    fn an_area_is_worked_by_hand_at_any_size() {
        for s in [1e-90, 1e-30, 1.0, 1e30, 1e80] {
            let t = [[0.0; 3], [s, 0.0, 0.0], [0.0, 2.0 * s, 0.0]];
            assert_eq!(area(t).to_bits(), (s * s).to_bits(), "{s}");
            let tilted = area([[0.0; 3], [s, 0.0, 0.0], [0.0, s, s]]);
            // A product, a division, a sum of squares, a root and a halving.
            let half_root_two = std::f64::consts::SQRT_2 / 2.0;
            assert!((tilted / (s * s) - half_root_two).abs() <= 4.0 * f64::EPSILON, "{s}");
        }
        assert_eq!(area([[1.0, 2.0, 3.0]; 3]).to_bits(), 0.0_f64.to_bits());
        assert_eq!(area([[0.0; 3], [1.0, 1.0, 1.0], [2.0, 2.0, 2.0]]).to_bits(), 0.0_f64.to_bits());
    }

    /// A triangle whose edges from its first corner, or whose distance to the point,
    /// overflow has no scaled form: its first corner is given.
    #[test]
    fn a_triangle_beyond_the_range_of_its_differences_gives_its_first_corner() {
        let bits = |p: Point| p.map(f64::to_bits);
        let wide = [[-1e308, 0.0, 0.0], [1e308, 0.0, 0.0], [0.0, 1.0, 0.0]];
        assert_eq!(bits(on_triangle([0.0, 5.0, 0.0], wide)), bits(wide[0]));
        let far = [[1e308, 0.0, 0.0], [1e308, 1.0, 0.0], [1e308, 0.0, 1.0]];
        assert_eq!(bits(on_triangle([-1e308, 0.5, 0.5], far)), bits(far[0]));
    }
}
