//! A mesh converted into a narrow-band signed distance field on brodo's tree
//! (ARCHITECTURE.md §5.1, ADR-0027).
//!
//! Voxel `i` along an axis spans `origin + h·i` to `origin + h·(i + 1)` in the mesh's world
//! coordinates and is sampled at its centre. Its value is the distance to the nearest
//! triangle, negative where the winding number is 1/2 or more, clamped to ±N·h and stored
//! as `f32`. A leaf-sized cell is a leaf when one of its values, so stored, is nearer zero
//! than N·h. Candidate cells are those whose centre lies within N·h and half a cell's
//! diagonal of a triangle, among the cells its box grown by N·h overlaps.
//!
//! Outside the leaves, regions are classified from the largest down, root region (4096³
//! voxels), upper node (128³), lower node (8³): a region holding no leaf holds no part of
//! the surface, so one winding number at its centre decides it, and an inside region
//! becomes one tile holding −N·h. Regions are looked at only where they overlap the mesh's
//! box grown by N·h; beyond it lies the background, +N·h. A mesh with no triangle converts
//! to the background alone.
//!
//! The same mesh and options give the same topology and the same field bytes whatever the
//! thread count (ARCHITECTURE.md §5.1).

use std::collections::BTreeSet;

use brodo::error::FieldError;
use brodo::field::{Field, uniform};
use brodo::meta::{GridMeta, SDF_KEY, SDF_VERSION};
use brodo::parallel::par_update;
use brodo::topology::Topology;
use brodo::tree::{Coord, TileLevel};

use crate::distance::{Point, on_triangle, squared};
use crate::error::ConvertError;
use crate::mesh::Mesh;
use crate::parity;
use crate::winding::{Accuracy, Winding};

/// Voxels along a leaf-sized cell's edge.
const CELL: i64 = 8;

/// How a point's sign is found: whether it is inside the mesh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Sign {
    /// Inside where the generalised winding number is 1/2 or more, at the options'
    /// accuracy. Holds where parts pass into each other or walls are doubled; a face turned
    /// in moves the inside.
    Winding,
    /// Inside where most of the lines through the point along the three axes cross the
    /// mesh an odd number of times, a line crossing an odd number of times not voting and a
    /// tie outside. Does not depend on the triangles' orientation and holds across small
    /// holes; fails where parts pass into each other or walls are doubled (ADR-0027).
    Parity,
}

/// Whether points are inside, by a sign.
pub(crate) struct Inside<'a> {
    winding: &'a Winding,
    sign: Sign,
    accuracy: Accuracy,
}

impl<'a> Inside<'a> {
    pub(crate) fn new(winding: &'a Winding, sign: Sign, accuracy: Accuracy) -> Inside<'a> {
        Inside { winding, sign, accuracy }
    }

    pub(crate) fn at(&self, p: Point) -> bool {
        self.at_distance(p, self.winding.nearest(p))
    }

    /// Whether `p`, at distance `d` from the mesh, is inside. A point on the mesh, at
    /// distance exactly 0, is inside whatever the sign asked for, as Hermite data takes it:
    /// on a face buried in another part, on any face of a box, and on a face two parts
    /// share alike, where the winding number would turn on a rounded zero's sign and
    /// parity on the side its lines are moved to.
    pub(crate) fn at_distance(&self, p: Point, d: Option<f64>) -> bool {
        if d == Some(0.0) {
            return true;
        }
        match self.sign {
            Sign::Winding => self.winding.number(p, self.accuracy) >= 0.5,
            Sign::Parity => parity::inside(self.winding.bvh(), p),
        }
    }
}

/// How a mesh is converted.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Options {
    /// A voxel's edge, h, in the mesh's unit: finite, and no less than `f32::MIN_POSITIVE`,
    /// so that the field's `f32` distances can hold it, or conversion is refused with
    /// `ConvertError::BadOption`.
    pub voxel_size: f64,
    /// Where voxel 0's corner lies, in the mesh's world coordinates: finite, or conversion
    /// is refused with `ConvertError::BadOption`.
    pub origin: Point,
    /// The band's half-width N, in voxels: at least 1, and N·h no more than `f32::MAX`,
    /// the background the field holds, or conversion is refused with
    /// `ConvertError::BadOption`.
    pub band: u32,
    /// How each winding number is found.
    pub accuracy: Accuracy,
    /// How each point's sign is found.
    pub sign: Sign,
    /// The grid's unit when the mesh states none.
    pub unit: Option<String>,
}

impl Options {
    /// Voxels of edge `voxel_size` on the world's lattice, a band of 3, and the winding
    /// number's sign by Barill et al.'s approximation with β = 2.
    pub fn new(voxel_size: f64) -> Options {
        Options {
            voxel_size,
            origin: [0.0; 3],
            band: 3,
            accuracy: Accuracy::Approximate { beta: 2.0 },
            sign: Sign::Winding,
            unit: None,
        }
    }

    /// The same options, voxel 0's corner at `origin`.
    #[must_use]
    pub fn with_origin(self, origin: Point) -> Options {
        Options { origin, ..self }
    }

    /// The same options, with a band of `band` voxels.
    #[must_use]
    pub fn with_band(self, band: u32) -> Options {
        Options { band, ..self }
    }

    /// The same options, winding numbers found with `accuracy`.
    #[must_use]
    pub fn with_accuracy(self, accuracy: Accuracy) -> Options {
        Options { accuracy, ..self }
    }

    /// The same options, each point's sign found by `sign`.
    #[must_use]
    pub fn with_sign(self, sign: Sign) -> Options {
        Options { sign, ..self }
    }

    /// The same options, `unit` the grid's unit when the mesh states none.
    #[must_use]
    pub fn with_unit(self, unit: Option<String>) -> Options {
        Options { unit, ..self }
    }

    fn check(&self) -> Result<(), ConvertError> {
        if !(self.voxel_size.is_finite() && self.voxel_size > 0.0) {
            return Err(ConvertError::BadOption("a voxel size that is not finite and positive"));
        }
        if !self.origin.iter().all(|c| c.is_finite()) {
            return Err(ConvertError::BadOption("an origin that is not finite"));
        }
        if self.band == 0 {
            return Err(ConvertError::BadOption("a band of no voxels"));
        }
        // The field holds f32 distances: a voxel's edge must be one, not rounded to zero
        // or below the normal range, and the band's half-width, the background, finite.
        if self.voxel_size < f64::from(f32::MIN_POSITIVE) {
            return Err(ConvertError::BadOption("a voxel size below f32's normal range"));
        }
        if f64::from(self.band) * self.voxel_size > f64::from(f32::MAX) {
            return Err(ConvertError::BadOption("a band's half-width beyond f32"));
        }
        if let Accuracy::Approximate { beta } = self.accuracy
            && !(beta.is_finite() && beta > 0.0)
        {
            return Err(ConvertError::BadOption("a β that is not finite and positive"));
        }
        Ok(())
    }

    /// A point in voxel index space, voxel corners at integers, as world coordinates.
    fn world(&self, p: Point) -> Point {
        [0, 1, 2].map(|k| self.origin[k] + self.voxel_size * p[k])
    }

    /// A world point in voxel index space.
    fn index(&self, p: Point) -> Point {
        [0, 1, 2].map(|k| (p[k] - self.origin[k]) / self.voxel_size)
    }
}

/// A mesh converted: its tree, its signed distances and the grid's metadata.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Converted {
    /// Leaves in the band, tiles inside.
    pub topology: Topology,
    /// Signed distances, negative inside, ±N·h beyond the band.
    pub field: Field<f32>,
    /// The voxel size, unit and transform from voxel to world coordinates.
    pub meta: GridMeta,
    /// The sign the conversion used, which Hermite data from it uses too.
    pub sign: Sign,
    /// The accuracy its winding numbers were found at, which Hermite data and the report's
    /// third layer use too.
    pub accuracy: Accuracy,
}

/// A conversion's lattice: voxel v's corner at `origin + h·v`, as conversion places it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Lattice {
    pub(crate) origin: Point,
    pub(crate) h: Point,
}

impl Lattice {
    /// Voxel `v`'s corner.
    pub(crate) fn world(&self, v: Coord) -> Point {
        self.at(v.map(f64::from))
    }

    /// The point at `v` in index space, given in `f64` so that it may lie beyond the `i32`
    /// range.
    pub(crate) fn at(&self, v: Point) -> Point {
        [0, 1, 2].map(|k| self.origin[k] + self.h[k] * v[k])
    }
}

impl Converted {
    /// Its lattice, read from its transform; none unless the transform is aligned with the
    /// axes, an entry off the diagonal 0 and not NaN, each voxel size positive and finite,
    /// and the origin finite.
    pub(crate) fn lattice(&self) -> Option<Lattice> {
        let (mut origin, mut h) = ([0.0; 3], [0.0; 3]);
        for (k, row) in self.meta.transform.iter().enumerate() {
            let off_axis = row.iter().take(3).enumerate().any(|(j, x)| j != k && *x != 0.0);
            let scale = row.get(k).copied().unwrap_or(0.0);
            let at = row.get(3).copied().unwrap_or(0.0);
            let positive = scale.partial_cmp(&0.0) == Some(std::cmp::Ordering::Greater);
            if off_axis || !positive || !scale.is_finite() || !at.is_finite() {
                return None;
            }
            (h[k], origin[k]) = (scale, at);
        }
        Some(Lattice { origin, h })
    }
}

/// Index space's corners `lo` and `hi`, as `i64` cells' range; refused beyond `i32`.
fn cell_range(lo: Point, hi: Point) -> Result<([i64; 3], [i64; 3]), ConvertError> {
    let limit = f64::from(i32::MAX) - 2.0 * 4096.0;
    if lo.iter().chain(&hi).any(|c| !c.is_finite() || c.abs() > limit) {
        return Err(ConvertError::OutOfRange);
    }
    // Voxels whose centre, index + 1/2, lies in [lo, hi].
    #[expect(clippy::cast_possible_truncation, reason = "within ±i32::MAX, checked above")]
    let first = lo.map(|c| ((c - 0.5).ceil() as i64).div_euclid(CELL));
    #[expect(clippy::cast_possible_truncation, reason = "within ±i32::MAX, checked above")]
    let last = hi.map(|c| ((c - 0.5).floor() as i64).div_euclid(CELL));
    Ok((first, last))
}

fn origin_of(cell: [i64; 3]) -> Result<Coord, ConvertError> {
    let c = |x: i64| i32::try_from(x * CELL).map_err(|_| ConvertError::OutOfRange);
    Ok([c(cell[0])?, c(cell[1])?, c(cell[2])?])
}

/// The candidate cells: near enough a triangle that a voxel of theirs may be in the band.
fn candidates(winding: &Winding, o: &Options) -> Result<BTreeSet<Coord>, ConvertError> {
    let band = f64::from(o.band);
    let reach = (band + 3.0_f64.sqrt() * 4.0) * o.voxel_size;
    let mut cells = BTreeSet::new();
    for t in winding.triangles() {
        let ix = t.map(|p| o.index(p));
        let lo = [0, 1, 2].map(|k| ix.iter().map(|p| p[k]).fold(f64::INFINITY, f64::min) - band);
        let hi =
            [0, 1, 2].map(|k| ix.iter().map(|p| p[k]).fold(f64::NEG_INFINITY, f64::max) + band);
        let (first, last) = cell_range(lo, hi)?;
        for z in first[2]..=last[2] {
            for y in first[1]..=last[1] {
                for x in first[0]..=last[0] {
                    let mid = [x, y, z].map(|c| (c * CELL) as f64 + 4.0);
                    let p = o.world(mid);
                    if squared(p, on_triangle(p, *t)) <= reach * reach {
                        cells.insert(origin_of([x, y, z])?);
                    }
                }
            }
        }
    }
    Ok(cells)
}

/// A voxel's signed distance, clamped to ±`width`. An inside voxel whose distance rounds
/// to zero holds −`f32::MIN_POSITIVE`, so that `v < 0` finds it inside.
fn value(winding: &Winding, inside: &Inside<'_>, o: &Options, v: Coord, width: f64) -> f32 {
    let p = o.world(v.map(|c| f64::from(c) + 0.5));
    let nearest = winding.nearest(p);
    let d = nearest.unwrap_or(f64::INFINITY);
    let is_inside = inside.at_distance(p, nearest);
    let signed = if is_inside { -d } else { d };
    #[expect(clippy::cast_possible_truncation, reason = "values are stored as f32 (ADR-0027)")]
    let stored = signed.clamp(-width, width) as f32;
    if is_inside && stored == 0.0 { -f32::MIN_POSITIVE } else { stored }
}

/// Converts `mesh` into a narrow-band signed distance field.
pub fn convert(mesh: &Mesh, o: &Options) -> Result<Converted, ConvertError> {
    o.check()?;
    let unit = mesh.unit.clone().or_else(|| o.unit.clone()).ok_or(ConvertError::NoUnit)?;
    let winding = Winding::new(mesh)?;
    let signs = Inside::new(&winding, o.sign, o.accuracy);
    let width = f64::from(o.band) * o.voxel_size;
    #[expect(clippy::cast_possible_truncation, reason = "values are stored as f32 (ADR-0027)")]
    let edge = width as f32;

    let empty = Topology::new();
    let mut e = empty.edit();
    for c in candidates(&winding, o)? {
        e.activate(c)?;
    }
    let (t, _) = e.commit()?;
    let mut f = Field::new(&t, edge);
    par_update(&t, &mut f, |n, values| {
        for (v, x) in t.leaf_voxels(n).map_err(|_| FieldError::UnknownLeaf(n.get()))?.zip(values) {
            *x = value(&winding, &signs, o, v, width);
        }
        Ok::<(), FieldError>(())
    })?;

    // Each step takes the last one's topology and field, so that they are freed as it ends.
    let (t, f) = trimmed(t, f, edge)?;
    let (t, f) = with_inside(t, f, &winding, &signs, o, edge)?;

    let h = o.voxel_size;
    let mut meta = GridMeta::new("sdf", [h; 3], &unit)?;
    for (row, origin) in meta.transform.iter_mut().zip(o.origin) {
        row[3] = origin;
    }
    meta.extra.insert(SDF_KEY.to_owned(), SDF_VERSION.to_owned());
    meta.check()?;
    Ok(Converted { topology: t, field: f, meta, sign: o.sign, accuracy: o.accuracy })
}

/// The topology and field without the candidates holding no value inside the band.
fn trimmed(t: Topology, f: Field<f32>, edge: f32) -> Result<(Topology, Field<f32>), ConvertError> {
    let mut e = t.edit();
    for n in t.leaf_numbers() {
        if f.leaf(&t, n)?.iter().all(|x| x.abs() >= edge) {
            e.deactivate(t.leaf_origin(n)?)?;
        }
    }
    let (t, remap) = e.commit()?;
    let f = f.remap(&remap, &uniform::<f32>)?;
    Ok((t, f))
}

/// The topology and field with the inside regions holding no leaf as tiles of −`edge`.
fn with_inside(
    t: Topology,
    f: Field<f32>,
    winding: &Winding,
    signs: &Inside<'_>,
    o: &Options,
    edge: f32,
) -> Result<(Topology, Field<f32>), ConvertError> {
    let mut e = t.edit();
    for (level, at) in inside(&t, winding, signs, o)? {
        e.add_tile(level, at)?;
    }
    let (t, remap) = e.commit()?;
    let mut f = f.remap(&remap, &uniform::<f32>)?;
    for n in t.tile_numbers().collect::<Vec<_>>() {
        f.set_tile(&t, n, -edge)?;
    }
    Ok((t, f))
}

/// The root regions, 4096³ voxels, overlapping cells `first` to `last`.
fn root_regions(first: [i64; 3], last: [i64; 3]) -> Result<Vec<Coord>, ConvertError> {
    let range = |k: usize| (first[k] * CELL).div_euclid(4096)..=(last[k] * CELL).div_euclid(4096);
    let at = |v: i64| i32::try_from(v * 4096).map_err(|_| ConvertError::OutOfRange);
    let mut out = Vec::new();
    for z in range(2) {
        for y in range(1) {
            for x in range(0) {
                out.push([at(x)?, at(y)?, at(z)?]);
            }
        }
    }
    Ok(out)
}

/// The inside regions holding no leaf, each at the largest level that holds none.
fn inside(
    t: &Topology,
    winding: &Winding,
    signs: &Inside<'_>,
    o: &Options,
) -> Result<Vec<(TileLevel, Coord)>, ConvertError> {
    let leaves: BTreeSet<Coord> =
        t.leaf_numbers().map(|n| t.leaf_origin(n)).collect::<Result<_, _>>()?;
    let align = |c: Coord, edge: i32| c.map(|x| x.div_euclid(edge) * edge);
    let uppers: BTreeSet<Coord> = leaves.iter().map(|c| align(*c, 128)).collect();
    let roots: BTreeSet<Coord> = leaves.iter().map(|c| align(*c, 4096)).collect();
    let Some((lo, hi)) = winding.bounds() else { return Ok(Vec::new()) };
    let band = f64::from(o.band);
    let (lo, hi) = (o.index(lo).map(|c| c - band), o.index(hi).map(|c| c + band));
    let (first, last) = cell_range(lo, hi)?;
    let overlaps = |at: Coord, edge: i64| {
        (0..3).all(|k| {
            let (a, b) = (i64::from(at[k]), i64::from(at[k]) + edge);
            a < last[k] * CELL + CELL && b > first[k] * CELL
        })
    };
    let mut stack: Vec<(TileLevel, Coord)> =
        root_regions(first, last)?.into_iter().map(|at| (TileLevel::Root, at)).collect();
    let mut out = Vec::new();
    while let Some((level, at)) = stack.pop() {
        let holds = match level {
            TileLevel::Root => roots.contains(&at),
            TileLevel::Upper => uppers.contains(&at),
            TileLevel::Lower => leaves.contains(&at),
        };
        if !holds {
            let half = f64::from(level.edge()) / 2.0;
            let centre = o.world(at.map(|c| f64::from(c) + half));
            if signs.at(centre) {
                out.push((level, at));
            }
            continue;
        }
        let (child, edge) = match level {
            TileLevel::Root => (TileLevel::Upper, 128),
            TileLevel::Upper => (TileLevel::Lower, 8),
            TileLevel::Lower => continue,
        };
        let n = level.edge() / edge;
        for i in (0..n * n * n).rev() {
            let c = [at[0] + i % n * edge, at[1] + i / n % n * edge, at[2] + i / (n * n) * edge];
            if overlaps(c, i64::from(edge)) {
                stack.push((child, c));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether `check` takes options with this voxel size and band.
    fn taken(voxel_size: f64, band: u32) -> bool {
        let o = Options { band, ..Options::new(voxel_size) };
        let checked = o.check();
        assert!(matches!(checked, Ok(()) | Err(ConvertError::BadOption(_))), "{checked:?}");
        checked.is_ok()
    }

    /// The voxel size and the band are refused beyond what `Options` documents and taken at
    /// its edge: f32's least normal voxel size, and a band's half-width of f32's largest
    /// value.
    #[test]
    fn options_are_taken_up_to_their_documented_limits() {
        for h in [0.0, -1.0, f64::INFINITY, f64::NAN] {
            assert!(!taken(h, 3), "{h}");
        }
        assert!(!taken(1.0, 0));
        let least = f64::from(f32::MIN_POSITIVE);
        assert!(taken(least, 1));
        assert!(!taken(least / 2.0, 1));
        let largest = f64::from(f32::MAX);
        assert!(taken(largest, 1));
        assert!(!taken(largest / 2.0, 3));
    }
}
