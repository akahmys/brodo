//! The squared distance from a point to a segment, worked as a reference shares no code
//! with `colino::distance`: the least of the distances to the ends and, where the foot of the
//! perpendicular falls between them, its length by the cross product, |(p − u) × (v − u)|²
//! over |v − u|², rather than by the point a clamped projection gives.

use colino::distance::Point;

pub fn squared_to_edge(p: Point, u: Point, v: Point) -> f64 {
    let d = [v[0] - u[0], v[1] - u[1], v[2] - u[2]];
    let w = [p[0] - u[0], p[1] - u[1], p[2] - u[2]];
    let to = |q: Point| (0..3).map(|k| (p[k] - q[k]) * (p[k] - q[k])).sum::<f64>();
    let ends = to(u).min(to(v));
    let length = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    let along = w[0] * d[0] + w[1] * d[1] + w[2] * d[2];
    if length <= 0.0 || along <= 0.0 || along >= length {
        return ends;
    }
    let c = [w[1] * d[2] - w[2] * d[1], w[2] * d[0] - w[0] * d[2], w[0] * d[1] - w[1] * d[0]];
    ends.min((c[0] * c[0] + c[1] * c[1] + c[2] * c[2]) / length)
}
