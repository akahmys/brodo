//! Hermite data from a mesh, in brodo's format (ARCHITECTURE.md §5.1, ADR-0030, ADR-0032):
//! signs on the corners of a conversion's voxels, and on each edge whose ends differ, where
//! the mesh crosses it, its normal and its group.
//!
//! A leaf point's sign is found as the conversion found its voxels' (`Converted::sign`):
//! the winding number's, at the conversion's accuracy, or the parity count's; a point at distance
//! zero from the mesh is inside. Tiles, inside regions, are inside, and the
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
use brodo::field::{Field, LEAF_VOXELS};
use brodo::hermite::{Crossing, Crossings, Hermite};
use brodo::meta::{GridMeta, SDF_KEY};
use brodo::parallel::{par_reduce, par_update};
use brodo::topology::{LeafNumber, Topology};
use brodo::tree::Coord;

use crate::bvh::Bvh;
use crate::convert::{Converted, Inside, Lattice};
use crate::distance::{Point, cross, has_area, sub, unit};
use crate::error::HermiteError;
use crate::intersect::crossing;
use crate::mesh::Mesh;
use crate::winding::Winding;

/// A mesh's Hermite data, with the count of crossed edges no triangle was hit on.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Produced {
    /// The set, in brodo's format.
    pub set: Hermite,
    /// The set's metadata, to write it with (`Hermite::entries`): named `hermite`, on the
    /// conversion's lattice, in its unit, without the SDF convention's key.
    pub meta: GridMeta,
    /// Crossed edges whose data came from the nearest triangle, none being hit.
    pub unhit: u64,
}

/// The sign and place of the point after `v` along `axis`. Beyond the `i32` range lies
/// the background, sign 0, as brodo takes it.
fn after(
    t: &Topology,
    signs: &Field<u8>,
    lattice: &Lattice,
    axis: usize,
    v: Coord,
) -> Result<(u8, Point), FieldError> {
    let mut w = v;
    if let Some(c) = v[axis].checked_add(1) {
        w[axis] = c;
        return Ok((signs.value_at(t, w)?, lattice.world(w)));
    }
    let mut p = v.map(f64::from);
    p[axis] += 1.0;
    Ok((0, lattice.at(p)))
}

/// The triangle's unit normal, by its corners' order; zero for one with no area.
fn normal(t: [Point; 3]) -> Point {
    let [a, b, c] = t;
    unit(cross(sub(b, a), sub(c, a))).unwrap_or([0.0; 3])
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
    // A triangle with no area is never crossed (`crossing`), so each one hit has a normal.
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
    let near = bvh.nearest_segment(a, b, has_area);
    near.map(|(fraction, _, triangle)| Found { fraction, triangle, hit: false })
}

/// Hermite data for `mesh` on its conversion `c`, signs by `c`'s sign and winding numbers at
/// its accuracy, as `c` was made.
pub fn hermite(mesh: &Mesh, c: &Converted) -> Result<Produced, HermiteError> {
    mesh.check_groups()?;
    let lattice = c.lattice().ok_or(HermiteError::Transform)?;
    // One tree of boxes serves the winding number, the crossings and the nearest triangles.
    let winding = Winding::new(mesh)?;
    let bvh = winding.bvh();
    let t = &c.topology;
    let signs = signs(t, &lattice, bvh, &Inside::new(&winding, c.sign, c.accuracy))?;
    let mut unhit = 0_u64;
    let mut axes = Vec::with_capacity(3);
    for axis in 0..3 {
        let (crossings, missed) = crossings(mesh, t, &lattice, bvh, &signs, axis)?;
        unhit += missed;
        axes.push(crossings);
    }
    let crossings = gathered(t, &axes)?;
    let set = Hermite::new(t.clone(), signs, crossings, mesh.group_names.clone()).map_err(|e| {
        if let brodo::error::HermiteError::Violation { at: Some(at), rule: Rule::Owner, .. } = e {
            HermiteError::SignOutsideBand(at)
        } else {
            HermiteError::Format(e)
        }
    })?;
    let mut meta = c.meta.clone();
    meta.name = "hermite".to_owned();
    meta.extra.remove(SDF_KEY);
    Ok(Produced { set, meta, unhit })
}

/// Each axis's crossings, in leaf and place order, built into a set's crossings leaf by leaf.
fn gathered(t: &Topology, axes: &[Vec<Placed>]) -> Result<Crossings, HermiteError> {
    let mut b = Crossings::builder(t);
    let mut rest: Vec<&[Placed]> = axes.iter().map(Vec::as_slice).collect();
    for n in t.leaf_numbers() {
        let mut lists: [Vec<(usize, Crossing)>; 3] = Default::default();
        for (list, axis) in lists.iter_mut().zip(rest.iter_mut()) {
            let here = axis.iter().take_while(|(m, _, _)| *m == n).count();
            let (mine, after) = axis.split_at(here);
            list.extend(mine.iter().map(|(_, i, c)| (*i, *c)));
            *axis = after;
        }
        let [x, y, z] = &lists;
        b.leaf(n, [x, y, z]).map_err(HermiteError::Format)?;
    }
    b.build().map_err(HermiteError::Format)
}

/// A crossing on a leaf point's edge: the leaf, the point's index in it, and the crossing.
type Placed = (LeafNumber, usize, Crossing);

/// The triangle with area holding `p`, the lowest-numbered of those at distance 0, if `p`
/// is on one: a triangle with none has no normal to give.
fn on_mesh(bvh: &Bvh, p: Point) -> Option<u32> {
    bvh.nearest_where(p, has_area).filter(|(d, _)| *d == 0.0).map(|(_, i)| i)
}

/// Each leaf point's sign, tiles inside, the background outside.
fn signs(
    t: &Topology,
    lattice: &Lattice,
    bvh: &Bvh,
    inside: &Inside<'_>,
) -> Result<Field<u8>, HermiteError> {
    let mut signs = Field::new(t, 0_u8);
    par_update(t, &mut signs, |n, values| {
        for (v, x) in voxels(t, n)?.zip(values) {
            let p = lattice.world(v);
            let d = bvh.nearest(p).map(|(d, _)| d);
            *x = u8::from(inside.at_distance(p, d));
        }
        Ok::<(), FieldError>(())
    })?;
    for n in t.tile_numbers() {
        signs.set_tile(t, n, 1)?;
    }
    Ok(signs)
}

/// The crossings on one axis's edges, in leaf and place order, and how many came from no
/// hit.
fn crossings(
    mesh: &Mesh,
    t: &Topology,
    lattice: &Lattice,
    bvh: &Bvh,
    signs: &Field<u8>,
    axis: usize,
) -> Result<(Vec<Placed>, u64), HermiteError> {
    // Each leaf's crossed edges, by voxel index, in leaf order: crossings are few beside the
    // leaves' voxels, so none is kept for an edge with none.
    let leaf = |n: LeafNumber, lower: &[u8; LEAF_VOXELS]| -> Result<Vec<_>, FieldError> {
        let mut out = Vec::new();
        for (i, (v, inside)) in voxels(t, n)?.zip(lower).enumerate() {
            let (far, b) = after(t, signs, lattice, axis, v)?;
            if far == *inside {
                continue;
            }
            let a = lattice.world(v);
            let lower_inside = *inside == 1;
            let Some(mut f) = find(bvh, a, b, lower_inside) else { continue };
            // An inside end on the mesh holds the crossing exactly, though the edge may only
            // graze the mesh there and hit nothing; it is not counted as unhit, and takes the
            // normal and group of the triangle holding it, not of one farther along.
            let (end, fraction) = if lower_inside { (a, 0.0) } else { (b, 1.0) };
            if let Some(triangle) = on_mesh(bvh, end) {
                f = Found { fraction, triangle, hit: true };
            }
            out.push((n, i, f));
        }
        Ok(out)
    };
    let mut found = par_reduce(t, signs, Ok(Vec::new()), leaf, |all, mine| {
        let (mut all, mine) = (all?, mine?);
        all.extend(mine);
        Ok::<_, FieldError>(all)
    })??;
    found.sort_unstable_by_key(|(n, i, _)| (*n, *i));
    let mut c = Vec::with_capacity(found.len());
    let mut unhit = 0_u64;
    for (n, i, f) in found {
        let inside = signs.leaf(t, n)?.get(i).copied().unwrap_or(0);
        let mut m = bvh.corners(f.triangle).map_or([0.0; 3], normal);
        // Out of the inside end: along +axis when the lower end is inside.
        let inward = if inside == 1 { m[axis] < 0.0 } else { m[axis] > 0.0 };
        if inward {
            m = m.map(|x| -x);
        }
        unhit += u64::from(!f.hit);
        #[expect(clippy::cast_possible_truncation, reason = "stored as f32 (ADR-0030)")]
        let (fraction, normal) = (f.fraction as f32, m.map(|x| x as f32));
        let group = mesh.group(usize::try_from(f.triangle).unwrap_or(usize::MAX));
        c.push((n, i, Crossing { fraction, normal, group }));
    }
    Ok((c, unhit))
}

fn voxels(t: &Topology, n: LeafNumber) -> Result<impl Iterator<Item = Coord>, FieldError> {
    t.leaf_voxels(n).map_err(|_| FieldError::UnknownLeaf(n.get()))
}
