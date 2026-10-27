//! P4's acceptance (ADR-0035), colino's half: code written as a user outside the crate
//! writes it, against every type that may grow. Each `match` names every variant there is
//! and ends in a wildcard arm: a `#[non_exhaustive]` enum requires that arm, and an enum
//! without the attribute makes it unreachable, which the gate's lints deny, so a type
//! losing the attribute fails the gate. Each struct a user builds is built by its
//! constructor and set by assignment, and each one a user reads is read by its fields,
//! which a field added later leaves working.

use colino::convert::{Options, Sign, convert};
use colino::error::{ConvertError, HermiteError, MeshError, ReadError, ReportError};
use colino::hermite::hermite;
use colino::mesh::{Limits, Mesh};
use colino::report::{Check, Place, Reference, ThinKind, report};
use colino::surface::Surface;
use colino::winding::Accuracy;

fn read_error(e: &ReadError) -> &'static str {
    match e {
        ReadError::NotStl(..) => "NotStl",
        ReadError::NotText => "NotText",
        ReadError::Unexpected { .. } => "Unexpected",
        ReadError::BadNumber { .. } => "BadNumber",
        ReadError::NotATriangle { .. } => "NotATriangle",
        ReadError::NotFinite(..) => "NotFinite",
        ReadError::Unsupported { .. } => "Unsupported",
        ReadError::UnknownStatement { .. } => "UnknownStatement",
        ReadError::BadNumberOnLine { .. } => "BadNumberOnLine",
        ReadError::BadVertex { .. } => "BadVertex",
        ReadError::BadFace { .. } => "BadFace",
        ReadError::NoVertex { .. } => "NoVertex",
        ReadError::Package(..) => "Package",
        ReadError::Compression(..) => "Compression",
        ReadError::Xml { .. } => "Xml",
        ReadError::Model { .. } => "Model",
        ReadError::UnknownElement { .. } => "UnknownElement",
        ReadError::RequiredExtension(..) => "RequiredExtension",
        ReadError::NoObject { .. } => "NoObject",
        ReadError::TooLarge { .. } => "TooLarge",
        _ => "later",
    }
}

fn mesh_error(e: &MeshError) -> &'static str {
    match e {
        MeshError::BadIndex { .. } => "BadIndex",
        MeshError::TooManyVertices => "TooManyVertices",
        MeshError::GroupCount { .. } => "GroupCount",
        MeshError::UnnamedGroup { .. } => "UnnamedGroup",
        MeshError::GroupZeroNamed => "GroupZeroNamed",
        MeshError::BadGroupName(..) => "BadGroupName",
        MeshError::NotFinite { .. } => "NotFinite",
        _ => "later",
    }
}

fn convert_error(e: &ConvertError) -> &'static str {
    match e {
        ConvertError::Mesh(..) => "Mesh",
        ConvertError::Tree(..) => "Tree",
        ConvertError::Field(..) => "Field",
        ConvertError::Meta(..) => "Meta",
        ConvertError::BadOption(..) => "BadOption",
        ConvertError::NoUnit => "NoUnit",
        ConvertError::OutOfRange => "OutOfRange",
        _ => "later",
    }
}

fn hermite_error(e: &HermiteError) -> &'static str {
    match e {
        HermiteError::Mesh(..) => "Mesh",
        HermiteError::Field(..) => "Field",
        HermiteError::Format(..) => "Format",
        HermiteError::Transform => "Transform",
        HermiteError::SignOutsideBand(..) => "SignOutsideBand",
        _ => "later",
    }
}

fn report_error(e: &ReportError) -> &'static str {
    match e {
        ReportError::Mesh(..) => "Mesh",
        ReportError::Analysis(..) => "Analysis",
        ReportError::Transform => "Transform",
        _ => "later",
    }
}

fn reference(e: &Reference) -> &'static str {
    match e {
        Reference::Closed { .. } => "Closed",
        Reference::None { .. } => "None",
        _ => "later",
    }
}

fn place(e: &Place) -> &'static str {
    match e {
        Place::Solid => "Solid",
        Place::Complement { .. } => "Complement",
        _ => "later",
    }
}

fn accuracy(e: &Accuracy) -> &'static str {
    match e {
        Accuracy::Exact => "Exact",
        Accuracy::Approximate { .. } => "Approximate",
        _ => "later",
    }
}

fn thin_kind(e: &ThinKind) -> &'static str {
    match e {
        ThinKind::Wall => "Wall",
        ThinKind::Gap => "Gap",
        _ => "later",
    }
}

fn check(e: &Check) -> &'static str {
    match e {
        Check::Same(i) => {
            assert!(i.euler <= i.solid + i.complement);
            "Same"
        }
        Check::Changed { coarse, fine } => {
            assert_ne!(
                (coarse.euler, coarse.solid, coarse.complement),
                (fine.euler, fine.solid, fine.complement)
            );
            "Changed"
        }
        Check::Unchecked => "Unchecked",
        _ => "later",
    }
}

fn sign(e: &Sign) -> &'static str {
    match e {
        Sign::Winding => "Winding",
        Sign::Parity => "Parity",
        _ => "later",
    }
}

/// A tetrahedron, its triangles facing out.
fn tetrahedron() -> Mesh {
    let vertices = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    Mesh::new(vertices, vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]])
}

#[test]
fn every_enum_that_may_grow_is_matched_with_a_wildcard() {
    assert_eq!(read_error(&ReadError::NotText), "NotText");
    let bad = Mesh::new(vec![[0.0; 3]], vec![[0, 1, 2]]);
    assert_eq!(mesh_error(&Surface::of(&bad).unwrap_err()), "BadIndex");
    let no_unit = convert(&tetrahedron(), &Options::new(0.25)).unwrap_err();
    assert_eq!(convert_error(&no_unit), "NoUnit");
    assert_eq!(hermite_error(&HermiteError::Transform), "Transform");
    assert_eq!(report_error(&ReportError::Transform), "Transform");
    assert_eq!(accuracy(&Accuracy::Exact), "Exact");
    assert_eq!(sign(&Sign::Parity), "Parity");
    assert_eq!(thin_kind(&ThinKind::Gap), "Gap");
    assert_eq!(check(&Check::Unchecked), "Unchecked");
    let mesh = tetrahedron().with_unit(Some("mm".to_owned()));
    let c = convert(&mesh, &Options::new(0.25)).unwrap();
    let r = report(&mesh, &c, &[("inside".to_owned(), [0.2, 0.2, 0.2])]).unwrap();
    assert_eq!(reference(&r.reference), "Closed");
    assert_eq!(place(&r.regions[0].place), "Solid");
}

#[test]
fn every_struct_is_built_by_its_constructor_set_by_assignment_and_read_by_its_fields() {
    let mut mesh = tetrahedron();
    mesh.unit = Some("mm".to_owned());
    let mut options = Options::new(0.25);
    options.band = 2;
    options.accuracy = Accuracy::Exact;
    options.sign = Sign::Parity;
    let mut limits = Limits::new(10, 10, 1000);
    limits.bytes = 2000;
    assert_eq!((limits.triangles, limits.bytes), (10, 2000));
    let c = convert(&mesh, &options).unwrap();
    assert_eq!((c.meta.unit.as_str(), c.topology.leaf_count() > 0), ("mm", true));
    assert_eq!(c.sign, Sign::Parity);
    assert!(c.field.value_at(&c.topology, [0, 0, 0]).unwrap() < 0.0);
    let produced = hermite(&mesh, &c, Accuracy::Exact).unwrap();
    assert_eq!((produced.unhit, produced.set.group_names().len()), (0, 0));
    let r = report(&mesh, &c, &[("inside".to_owned(), [0.2, 0.2, 0.2])]).unwrap();
    assert_eq!((r.solid.components, r.euler, r.regions[0].name.as_str()), (1, 1, "inside"));
    assert!(r.located);
    for t in &r.thin {
        assert!(t.least < 0.5 && t.points > 0 && t.lo[0] <= t.hi[0], "{:?}", t.kind);
        assert_ne!(check(&t.check), "later");
    }
    let surface = Surface::of(&mesh).unwrap();
    assert_eq!((surface.vertices, surface.edges, surface.faces, surface.open_edges), (4, 6, 4, 0));
}
