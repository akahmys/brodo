//! The topology report's first two layers (ARCHITECTURE.md §5.2, ADR-0028).
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
//! Where a change lies is not known to these layers: `located` is false until the third.

use brodo::analysis::{Connectivity, Label, Labels, label};
use brodo::euler::euler;
use brodo::tree::Coord;

use crate::convert::Converted;
use crate::distance::Point;
use crate::error::ReportError;
use crate::mesh::Mesh;
use crate::surface::Surface;

/// Betti numbers: components, tunnels and cavities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
pub struct Region {
    /// The name the caller gave.
    pub name: String,
    /// The point, in world coordinates.
    pub point: Point,
    /// Where it lies.
    pub place: Place,
}

/// The topology report's first two layers.
#[derive(Debug, Clone, PartialEq)]
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
    /// Whether changes are located; not until the third layer.
    pub located: bool,
}

fn solid(v: f32) -> bool {
    v < 0.0
}

/// The voxel holding a world point, or nothing beyond the `i32` range.
fn voxel(c: &Converted, p: Point) -> Result<Option<Coord>, ReportError> {
    let t = c.meta.transform;
    let mut out = [0_i32; 3];
    for (k, o) in out.iter_mut().enumerate() {
        let row = t.get(k).ok_or(ReportError::Transform)?;
        let h = row.get(k).copied().unwrap_or(0.0);
        let off_axis = row.iter().take(3).enumerate().any(|(j, x)| j != k && x.abs() > 0.0);
        if off_axis || h.is_nan() || h <= 0.0 {
            return Err(ReportError::Transform);
        }
        let index =
            ((p.get(k).copied().unwrap_or(0.0) - row.get(3).copied().unwrap_or(0.0)) / h).floor();
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
        located: false,
    })
}
