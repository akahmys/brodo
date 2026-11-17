//! Reference tier (TESTING.md §1): a conversion written to a cache file and read back, rebuilt
//! by `Converted::from_parts` with the sign and accuracy it was made with, gives the same
//! Hermite bytes and the same report as the conversion itself (issue 9); parts that are not
//! a conversion's are refused.

use brodo::cache::{CacheEntry, from_bytes, to_bytes};
use brodo::error::FieldError;
use brodo::field::{AnyField, Field};
use brodo::meta::{SDF_KEY, SDF_VERSION};
use brodo::topology::Topology;
use colino::convert::{Converted, Options, Sign, convert};
use colino::error::ConvertError;
use colino::hermite::hermite;
use colino::mesh::Mesh;
use colino::report::report;
use colino::winding::Accuracy;

mod common;

/// `mesh` in millimetres, converted by parity at an approximate accuracy, neither the
/// default, so that a rebuilt conversion that dropped them would differ.
fn converted(mesh: Mesh) -> (Mesh, Converted) {
    let mesh = mesh.with_unit(Some("mm".to_owned()));
    let mut options = Options::new(0.125);
    options.sign = Sign::Parity;
    options.accuracy = Accuracy::Approximate { beta: 3.0 };
    let c = convert(&mesh, &options).unwrap();
    (mesh, c)
}

/// `c` written to a cache file, read back and thawed.
fn read_back(c: &Converted) -> (Topology, Field<f32>, brodo::meta::GridMeta) {
    let entry = CacheEntry::new(&c.meta, &c.topology, &c.field).unwrap();
    let bytes = to_bytes(&[entry]).unwrap();
    let file = from_bytes(&bytes).unwrap();
    let grid = &file.grids()[0];
    let (t, f) = grid.thaw(u64::MAX).unwrap();
    let AnyField::F32(f) = f else { panic!("not f32") };
    (t, f, grid.meta().clone())
}

#[test]
fn a_conversion_read_back_gives_the_same_hermite_data_and_report() {
    let (mesh, c) = converted(common::torus(2.0, 0.7, 16));
    let (t, f, meta) = read_back(&c);
    let back = Converted::from_parts(t, f, meta, c.sign, c.accuracy).unwrap();
    assert_eq!((back.sign, back.accuracy), (Sign::Parity, Accuracy::Approximate { beta: 3.0 }));
    let hermite_bytes = |c: &Converted| {
        let p = hermite(&mesh, c).unwrap();
        (to_bytes(&p.set.entries("h", &p.meta).unwrap()).unwrap(), p.unhit)
    };
    assert_eq!(hermite_bytes(&back), hermite_bytes(&c));
    let points = [("hole".to_owned(), [0.0, 0.0, 0.0]), ("ring".to_owned(), [2.0, 0.0, 0.0])];
    let reported = |c: &Converted| format!("{:?}", report(&mesh, c, &points).unwrap());
    assert_eq!(reported(&back), reported(&c));
}

#[test]
fn parts_that_are_not_a_conversions_are_refused() {
    let (_, c) = converted(common::sphere(2, 1.0, [0.1, 0.2, 0.3]));
    let parts = || (c.topology.clone(), c.field.clone(), c.meta.clone());
    let build = |(t, f, meta): (Topology, Field<f32>, _), accuracy| {
        Converted::from_parts(t, f, meta, Sign::Winding, accuracy)
    };
    assert!(build(parts(), Accuracy::Exact).is_ok());

    let (t, f, mut meta) = parts();
    meta.extra.remove(SDF_KEY);
    assert_eq!(build((t, f, meta), Accuracy::Exact).unwrap_err(), ConvertError::NotSdf);
    let (t, f, mut meta) = parts();
    meta.extra.insert(SDF_KEY.to_owned(), format!("{SDF_VERSION}0"));
    assert_eq!(build((t, f, meta), Accuracy::Exact).unwrap_err(), ConvertError::NotSdf);

    let (_, f, meta) = parts();
    let (other, _) = c.topology.edit().commit().unwrap();
    assert_eq!(
        build((other, f, meta), Accuracy::Exact).unwrap_err(),
        ConvertError::Field(FieldError::StaleField)
    );

    let (t, f, mut meta) = parts();
    meta.unit.clear();
    assert!(matches!(build((t, f, meta), Accuracy::Exact), Err(ConvertError::Meta(_))));

    for beta in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let refused = build(parts(), Accuracy::Approximate { beta }).unwrap_err();
        assert!(matches!(refused, ConvertError::BadOption(_)), "{beta}");
    }
}
