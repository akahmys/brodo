//! Hermite data from a mesh, in brodo's format (ARCHITECTURE.md §5.1, ADR-0030, ADR-0032):
//! signs on the corners of a conversion's voxels, and on each edge whose ends differ, where
//! the mesh crosses it, its normal and its group.
//!
//! A leaf point's sign is the winding number's, at the conversion's accuracy; a point at
//! distance zero from the mesh is inside. Tiles, inside regions, are inside, and the
//! background outside. Signs are found first, then each edge whose stored signs differ is
//! tested against every triangle with area near it (`intersect::crossing`): the hit nearest
//! the edge's inside end is kept, the lowest triangle number of equals, and at an inside end
//! on the mesh the crossing lies exactly there, whether or not the edge, grazing the mesh,
//! hits it. Where no triangle is hit otherwise, near a hole, an overlap, or by the
//! approximate winding number, the triangle with area nearest the edge gives the data, at
//! the edge's point nearest it; such edges are counted. The normal is the
//! triangle's, turned to point from the inside end out where it is not perpendicular to the
//! edge; the group is the triangle's. Values are found in `f64` and stored in `f32`, each
//! point and edge on its own, so the bytes do not depend on the thread count.
//!
//! A badly oriented mesh can move the winding number's inside away from the mesh: within
//! the band its false surface shows as unhit edges; beyond it, signs would differ across
//! an edge no leaf point owns, and the mesh is refused (ADR-0030, B-016).

use brodo::error::{FieldError, Rule};
use brodo::field::Field;
use brodo::hermite::{Crossings, Hermite, NO_CROSSING};
use brodo::parallel::par_update;
use brodo::topology::{LeafNumber, Topology};
use brodo::tree::Coord;

use crate::bvh::Bvh;
use crate::convert::Converted;
use crate::distance::{Point, cross, dot, sub};
use crate::error::HermiteError;
use crate::intersect::{crossing, nearest};
use crate::mesh::Mesh;
use crate::winding::{Accuracy, Winding};

/// A mesh's Hermite data, with the count of crossed edges no triangle was hit on.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Produced {
    /// The set, in brodo's format.
    pub set: Hermite,
    /// Crossed edges whose data came from the nearest triangle, none being hit.
    pub unhit: u64,
}

/// The grid's lattice: point v at `origin + h·v`, as conversion places voxel corners.
struct Lattice {
    origin: Point,
    h: Point,
}

impl Lattice {
    fn of(c: &Converted) -> Result<Lattice, HermiteError> {
        let (mut origin, mut h) = ([0.0; 3], [0.0; 3]);
        for (k, row) in c.meta.transform.iter().enumerate() {
            let off_axis = row.iter().take(3).enumerate().any(|(j, x)| j != k && *x != 0.0);
            let scale = row.get(k).copied().unwrap_or(0.0);
            if off_axis || scale.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
                return Err(HermiteError::Transform);
            }
            (h[k], origin[k]) = (scale, row.get(3).copied().unwrap_or(0.0));
        }
        Ok(Lattice { origin, h })
    }

    fn world(&self, v: Coord) -> Point {
        [0, 1, 2].map(|k| self.origin[k] + self.h[k] * f64::from(v[k]))
    }
}

fn step(axis: usize, v: Coord) -> Coord {
    let mut w = v;
    w[axis] += 1;
    w
}

/// The triangle's unit normal, by its corners' order; zero for one with no area.
fn normal(t: [Point; 3]) -> Point {
    let [a, b, c] = t;
    let n = cross(sub(b, a), sub(c, a));
    let length = dot(n, n).sqrt();
    if length > 0.0 { n.map(|x| x / length) } else { [0.0; 3] }
}

fn has_area(t: [Point; 3]) -> bool {
    normal(t).iter().any(|x| *x != 0.0)
}

/// What the search finds on one edge: its fraction, its triangle, and whether one was hit.
#[derive(Debug, Clone, Copy)]
struct Found {
    fraction: f64,
    triangle: u32,
    hit: bool,
}

/// The crossing on the edge from `a` to `b`, the lower end inside when `lower_inside`.
fn find(bvh: &Bvh, a: Point, b: Point, lower_inside: bool) -> Option<Found> {
    let mut best: Option<(f64, u32)> = None;
    // A triangle with no area is never crossed: the test's determinant is 0 (Woop et al.
    // §3.1).
    for i in bvh.near_segment(a, b) {
        let Some(t) = bvh.corners(i) else { continue };
        let Some(s) = crossing(a, b, t) else { continue };
        // Nearest the inside end; candidates come in ascending order, so the first of
        // equals stays.
        let better = best.is_none_or(|(b, _)| if lower_inside { s < b } else { s > b });
        if better {
            best = Some((s, i));
        }
    }
    if let Some((fraction, triangle)) = best {
        return Some(Found { fraction, triangle, hit: true });
    }
    // None hit: the triangle with area nearest the edge, the lowest of equals.
    let mut near: Option<(f64, f64, u32)> = None;
    for i in 0..bvh.len() {
        let Some(t) = bvh.corners(i).filter(|t| has_area(*t)) else { continue };
        let (s, d) = nearest(a, b, t);
        if near.is_none_or(|(_, e, _)| d < e) {
            near = Some((s, d, i));
        }
    }
    near.map(|(fraction, _, triangle)| Found { fraction, triangle, hit: false })
}

/// Hermite data for `mesh` on its conversion `c`, signs at `accuracy`, as `c` was made.
pub fn hermite(mesh: &Mesh, c: &Converted, accuracy: Accuracy) -> Result<Produced, HermiteError> {
    mesh.check_groups()?;
    let lattice = Lattice::of(c)?;
    let bvh = Bvh::new(mesh)?;
    let t = &c.topology;
    let signs = signs(mesh, t, &lattice, &bvh, accuracy)?;
    let mut unhit = 0_u64;
    let mut axes = Vec::with_capacity(3);
    for axis in 0..3 {
        let (crossings, missed) = crossings(mesh, t, &lattice, &bvh, &signs, axis)?;
        unhit += missed;
        axes.push(crossings);
    }
    let [x, y, z]: [Crossings; 3] = axes.try_into().map_err(|_| HermiteError::Transform)?;
    let set = Hermite::new(t.clone(), signs, [x, y, z], mesh.group_names.clone()).map_err(|e| {
        if let brodo::error::HermiteError::Violation { at: Some(at), rule: Rule::Owner, .. } = e {
            HermiteError::SignOutsideBand(at)
        } else {
            HermiteError::Format(e)
        }
    })?;
    Ok(Produced { set, unhit })
}

fn on_mesh(bvh: &Bvh, p: Point) -> bool {
    bvh.nearest(p).is_some_and(|(d, _)| d == 0.0)
}

/// Each leaf point's sign, tiles inside, the background outside.
fn signs(
    mesh: &Mesh,
    t: &Topology,
    lattice: &Lattice,
    bvh: &Bvh,
    accuracy: Accuracy,
) -> Result<Field<u8>, HermiteError> {
    let winding = Winding::new(mesh)?;
    let mut signs = Field::new(t, 0_u8);
    par_update(t, &mut signs, |n, values| {
        for (v, x) in voxels(t, n)?.zip(values) {
            let p = lattice.world(v);
            *x = u8::from(on_mesh(bvh, p) || winding.number(p, accuracy) >= 0.5);
        }
        Ok::<(), FieldError>(())
    })?;
    for n in t.tile_numbers() {
        signs.set_tile(t, n, 1)?;
    }
    Ok(signs)
}

/// The crossings on one axis's edges, and how many came from no hit.
fn crossings(
    mesh: &Mesh,
    t: &Topology,
    lattice: &Lattice,
    bvh: &Bvh,
    signs: &Field<u8>,
    axis: usize,
) -> Result<(Crossings, u64), HermiteError> {
    // Each edge's fraction, its triangle's number as bits, and 1 where it was hit; a
    // fraction of −1 where it has no crossing.
    let mut found = Field::new(t, [-1.0_f64, 0.0, 0.0]);
    par_update(t, &mut found, |n, values| {
        let lower = signs.leaf(t, n)?;
        for ((v, x), inside) in voxels(t, n)?.zip(values).zip(lower) {
            let w = step(axis, v);
            if signs.value_at(t, w)? == *inside {
                continue;
            }
            let (a, b) = (lattice.world(v), lattice.world(w));
            let lower_inside = *inside == 1;
            let Some(mut f) = find(bvh, a, b, lower_inside) else { continue };
            // An inside end on the mesh holds the crossing exactly, though the edge may only
            // graze the mesh there and hit nothing; it is not counted as unhit.
            if lower_inside && on_mesh(bvh, a) {
                (f.fraction, f.hit) = (0.0, true);
            } else if !lower_inside && on_mesh(bvh, b) {
                (f.fraction, f.hit) = (1.0, true);
            }
            let hit = if f.hit { 1.0 } else { 0.0 };
            *x = [f.fraction, f64::from_bits(u64::from(f.triangle)), hit];
        }
        Ok::<(), FieldError>(())
    })?;
    let mut c = Crossings {
        fraction: Field::new(t, NO_CROSSING.0),
        normal: Field::new(t, NO_CROSSING.1),
        group: Field::new(t, NO_CROSSING.2),
    };
    let mut unhit = 0_u64;
    for n in t.leaf_numbers() {
        let (lower, found) = (signs.leaf(t, n)?, found.leaf(t, n)?);
        for (i, ([fraction, triangle, hit], inside)) in found.iter().zip(lower).enumerate() {
            if *fraction < 0.0 {
                continue;
            }
            let triangle = u32::try_from(triangle.to_bits()).unwrap_or(u32::MAX);
            let mut m = bvh.corners(triangle).map_or([0.0; 3], normal);
            // Out of the inside end: along +axis when the lower end is inside.
            let inward = if *inside == 1 { m[axis] < 0.0 } else { m[axis] > 0.0 };
            if inward {
                m = m.map(|x| -x);
            }
            unhit += u64::from(*hit == 0.0);
            #[expect(clippy::cast_possible_truncation, reason = "stored as f32 (ADR-0030)")]
            {
                c.fraction.leaf_mut(t, n)?[i] = *fraction as f32;
                c.normal.leaf_mut(t, n)?[i] = m.map(|x| x as f32);
            }
            let group = mesh.group(usize::try_from(triangle).unwrap_or(usize::MAX));
            c.group.leaf_mut(t, n)?[i] = group;
        }
    }
    Ok((c, unhit))
}

fn voxels(t: &Topology, n: LeafNumber) -> Result<impl Iterator<Item = Coord>, FieldError> {
    t.leaf_voxels(n).map_err(|_| FieldError::UnknownLeaf(n.get()))
}
