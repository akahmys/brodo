//! The topology report's three layers (ARCHITECTURE.md §5.2, ADR-0028).
//!
//! Layer 1, global invariants: the solid is φ < 0 under 26-connectivity, its complement
//! φ ≥ 0 under 6. The solid's β₀ counts its components, β₂ the complement's components that
//! do not reach the outside, and β₁ = β₀ + β₂ − χ. The complement is taken in the 3-sphere,
//! the outside closed by a point at infinity: its β₀ is counted, its components, which
//! duality makes 1 + the solid's β₂; its β₁ is the solid's, and its β₂ the solid's β₀ less
//! one, or 1 when there is no solid. Under the 26/6 pairing each lattice vertex adds as
//! much to the complement's χ as to the solid's, so only β₀ is a count of its own.
//!
//! The reference is the input's V − E + F when, welded, it is a closed 2-manifold: the
//! solid's χ is then half the surface's, and a grid whose χ differs is reported with the
//! difference. A surface that crosses itself passes as closed, so its reference counts the
//! pieces as if apart: a difference there may be the input's own. An input that is not a
//! closed 2-manifold has no reference; the surface's counts say why.
//!
//! Layer 2, labelled connectivity: each named point is answered with the complement
//! component of the voxel holding it, or the solid; pairs of points in one component and
//! pairs apart are listed. A point beyond the tree lies outside.
//!
//! Layer 3, local risk: places thinner, or gaps narrower, than two voxels, found from the
//! mesh along its triangles' normals (`colino::thin`), each with its box; there a
//! conversion may have opened a wall or closed a gap. Sides are found by the conversion's
//! sign, at its accuracy (`Converted::accuracy`). Each place is checked by its box's
//! invariants at h and at h/2, sampled by that sign.

use brodo::analysis::{Connectivity, Label, Labels, label};
use brodo::euler::euler;
use brodo::tree::Coord;

use crate::convert::{Converted, Inside, Lattice};
use crate::distance::Point;
use crate::error::ReportError;
use crate::mesh::Mesh;
use crate::surface::Surface;
use crate::thin::{check, resolves, thin};
use crate::winding::Winding;

/// Betti numbers: components, tunnels and cavities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Betti {
    /// β₀.
    pub components: i64,
    /// β₁.
    pub tunnels: i64,
    /// β₂.
    pub cavities: i64,
}

/// What the input says the solid's Euler characteristic is, if it can.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum Reference {
    /// The welded input is a closed 2-manifold: half its V − E + F.
    Closed {
        /// The surface's V − E + F.
        surface_euler: i64,
        /// The solid's, half the surface's.
        solid_euler: i64,
    },
    /// The welded input is not a closed 2-manifold; its counts say why.
    None {
        /// The surface, counted.
        surface: Surface,
    },
}

/// Two regions, by their place in [`Report::regions`], the earlier first.
pub type Pair = (usize, usize);

/// Where a named point lies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum Place {
    /// In the solid.
    Solid,
    /// In a component of the complement, numbered as the labelling numbers it.
    Complement {
        /// The component's number.
        component: u32,
        /// Whether it reaches the outside.
        outside: bool,
    },
}

/// A named point and where it lies.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct Region {
    /// The name the caller gave.
    pub name: String,
    /// The point, in world coordinates.
    pub point: Point,
    /// Where it lies.
    pub place: Place,
}

/// What a thin place is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum ThinKind {
    /// The solid is thinner there than two voxels: a conversion may open a hole in it.
    Wall,
    /// The space between the mesh's sides is narrower there than two voxels: a conversion
    /// may close it.
    Gap,
}

/// Invariants of the solid in a place's box: its χ, its components under 26-connectivity,
/// and the components of the rest, the space beyond the box with it, under 6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct Invariants {
    /// χ of the union of the solid's closed voxels.
    pub euler: i64,
    /// The solid's components.
    pub solid: i64,
    /// The rest's components.
    pub complement: i64,
}

/// What reconverting a place at h/2 says (ADR-0028).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum Check {
    /// The invariants at h are those at h/2.
    Same(Invariants),
    /// They differ: the conversion has changed the topology there.
    Changed {
        /// At h, the conversion's.
        coarse: Invariants,
        /// At h/2.
        fine: Invariants,
    },
    /// The place's box holds more voxels at h/2 than are checked.
    Unchecked,
}

/// A place thinner, or a gap narrower, than two voxels.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct Thin {
    /// A wall or a gap.
    pub kind: ThinKind,
    /// The least corner of the box holding its thin points, in world coordinates.
    pub lo: Point,
    /// The greatest corner.
    pub hi: Point,
    /// Its least thickness or width, in the mesh's unit: the least, over its thin points,
    /// of the distance from a point along its triangle's normal to the next triangle, to
    /// within 2⁻⁸ h, h the voxel size; where within that it lies is not promised. A
    /// thickness no sampled point reaches along its normal is not measured.
    pub least: f64,
    /// The thin points it holds, of those sampled h/2 apart.
    pub points: u64,
    /// Whether the conversion changed the topology there.
    pub check: Check,
}

/// The topology report's three layers.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct Report {
    /// The solid's Betti numbers.
    pub solid: Betti,
    /// The complement's in the 3-sphere: β₀ counted, β₁ and β₂ by duality.
    pub complement: Betti,
    /// The solid's Euler characteristic on the grid.
    pub euler: i64,
    /// The input's reference.
    pub reference: Reference,
    /// The grid's χ less the reference's, when there is a reference: nonzero when the
    /// conversion may have changed the topology.
    pub euler_change: Option<i64>,
    /// The named points, in the order given.
    pub regions: Vec<Region>,
    /// Pairs of regions, by their place in `regions`, in one complement component.
    pub together: Vec<Pair>,
    /// Pairs of regions in the complement, in different components.
    pub apart: Vec<Pair>,
    /// The thin walls, then the narrow gaps, each in the order the mesh's triangles first
    /// reach it.
    pub thin: Vec<Thin>,
    /// Whether places where a change may lie are located: true since the third layer.
    pub located: bool,
}

/// The grid's lattice, refused unless its transform is the axis-aligned one conversion
/// writes.
fn lattice(c: &Converted) -> Result<Lattice, ReportError> {
    c.lattice().ok_or(ReportError::Transform)
}

/// The grid's voxel size, the least of its axes'.
fn voxel_size(c: &Converted) -> Result<f64, ReportError> {
    Ok(lattice(c)?.h.into_iter().fold(f64::INFINITY, f64::min))
}

fn solid(v: f32) -> bool {
    v < 0.0
}

/// The voxel holding a world point, or nothing beyond the `i32` range.
fn voxel(c: &Converted, p: Point) -> Result<Option<Coord>, ReportError> {
    let l = lattice(c)?;
    let mut out = [0_i32; 3];
    for (k, o) in out.iter_mut().enumerate() {
        let index = ((p[k] - l.origin[k]) / l.h[k]).floor();
        if !(f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&index) {
            return Ok(None);
        }
        #[expect(clippy::cast_possible_truncation, reason = "within i32, checked above")]
        let i = index as i32;
        *o = i;
    }
    Ok(Some(out))
}

fn place(c: &Converted, complement: &Labels, p: Point) -> Result<Place, ReportError> {
    let Some(v) = voxel(c, p)? else {
        return Ok(Place::Complement { component: outside_component(complement), outside: true });
    };
    // The complement's labelling leaves out exactly the solid's voxels.
    Ok(match complement.label_at(&c.topology, v)? {
        Label::Component(component) => Place::Complement {
            component,
            outside: complement.reaches_outside(Label::Component(component)),
        },
        Label::Excluded => Place::Solid,
    })
}

/// The complement component that reaches the outside, or the next number if none does.
fn outside_component(complement: &Labels) -> u32 {
    (0..complement.count())
        .find(|k| complement.reaches_outside(Label::Component(*k)))
        .unwrap_or(complement.count())
}

/// The input's reference, from its welded surface.
fn reference(mesh: &Mesh) -> Result<Reference, ReportError> {
    let surface = Surface::of(&mesh.welded()?)?;
    Ok(if surface.closed_manifold() {
        Reference::Closed { surface_euler: surface.euler(), solid_euler: surface.euler() / 2 }
    } else {
        Reference::None { surface }
    })
}

/// Pairs of regions in the complement: those in one component, and those apart.
fn pairs(regions: &[Region]) -> [Vec<Pair>; 2] {
    let (mut together, mut apart) = (Vec::new(), Vec::new());
    for (i, a) in regions.iter().enumerate() {
        for (j, b) in regions.iter().enumerate().skip(i + 1) {
            if let (
                Place::Complement { component: x, .. },
                Place::Complement { component: y, .. },
            ) = (a.place, b.place)
            {
                if x == y { together.push((i, j)) } else { apart.push((i, j)) }
            }
        }
    }
    [together, apart]
}

/// The third layer: the thin places of `mesh`, each checked in `converted`.
fn third_layer(mesh: &Mesh, converted: &Converted) -> Result<Vec<Thin>, ReportError> {
    let winding = Winding::new(mesh)?;
    let inside = Inside::new(&winding, converted.sign, converted.accuracy);
    let h = voxel_size(converted)?;
    if !resolves(winding.bvh(), h) {
        return Err(ReportError::TooFar);
    }
    let mut places = thin(winding.bvh(), &inside, h);
    for place in &mut places {
        place.check = check(converted, &inside, place)?;
    }
    Ok(places)
}

/// The report on `converted`, the conversion of `mesh`, with the named points `regions`.
pub fn report(
    mesh: &Mesh,
    converted: &Converted,
    regions: &[(String, Point)],
) -> Result<Report, ReportError> {
    let (t, f) = (&converted.topology, &converted.field);
    let solid_labels = label(t, f, solid, Connectivity::TwentySix)?;
    let complement = label(t, f, |v| !solid(v), Connectivity::Six)?;
    let count = |n: u32| i64::from(n);
    let reaching = (0..complement.count())
        .filter(|k| complement.reaches_outside(Label::Component(*k)))
        .count();
    let components = count(solid_labels.count());
    let cavities = count(complement.count()) - i64::try_from(reaching).unwrap_or(i64::MAX);
    let chi = euler(t, f, solid, Connectivity::TwentySix)?;
    let tunnels = components + cavities - chi;
    let reference = reference(mesh)?;
    let euler_change = match reference {
        Reference::Closed { solid_euler, .. } => Some(chi - solid_euler),
        Reference::None { .. } => None,
    };
    let regions = regions
        .iter()
        .map(|(name, point)| {
            Ok(Region {
                name: name.clone(),
                point: *point,
                place: place(converted, &complement, *point)?,
            })
        })
        .collect::<Result<Vec<_>, ReportError>>()?;
    let [together, apart] = pairs(&regions);
    let thin = third_layer(mesh, converted)?;
    Ok(Report {
        solid: Betti { components, tunnels, cavities },
        complement: Betti {
            components: count(complement.count()),
            tunnels,
            cavities: if components == 0 { 1 } else { components - 1 },
        },
        euler: chi,
        reference,
        euler_change,
        regions,
        together,
        apart,
        thin,
        located: true,
    })
}
