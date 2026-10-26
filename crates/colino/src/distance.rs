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

/// The squared distance between two points.
pub fn squared(a: Point, b: Point) -> f64 {
    let d = sub(a, b);
    dot(d, d)
}

/// The point of the segment from `a` to `b` nearest `p`.
pub fn on_segment(p: Point, a: Point, b: Point) -> Point {
    let ab = sub(b, a);
    let length = dot(ab, ab);
    if length <= 0.0 {
        return a;
    }
    along(a, b, (dot(sub(p, a), ab) / length).clamp(0.0, 1.0))
}

/// The point of the triangle `t` nearest `p`.
pub fn on_triangle(p: Point, t: [Point; 3]) -> Point {
    let [a, b, c] = t;
    let (ab, ac, ap) = (sub(b, a), sub(c, a), sub(p, a));
    let n = cross(ab, ac);
    if dot(n, n) <= 0.0 {
        let candidates = [on_segment(p, a, b), on_segment(p, b, c), on_segment(p, c, a)];
        return candidates
            .into_iter()
            .fold(a, |best, q| if squared(p, q) < squared(p, best) { q } else { best });
    }
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return along(a, b, d1 / (d1 - d3));
    }
    let cp = sub(p, c);
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
    let denom = 1.0 / (va + vb + vc);
    let (v, w) = (vb * denom, vc * denom);
    [a[0] + ab[0] * v + ac[0] * w, a[1] + ab[1] * v + ac[1] * w, a[2] + ab[2] * v + ac[2] * w]
}
