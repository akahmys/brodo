//! Format tier (TESTING.md §1) for the report as plain data (ADR-0028): behind the feature
//! `serde`, reports with every kind of place, a tunnel and both kinds of reference are
//! written as JSON and read back unchanged, under the field names they are written with.
#![cfg(feature = "serde")]

use colino::convert::{Options, convert};
use colino::mesh::Mesh;
use colino::report::{Report, report};

mod common;
use common::{sphere, torus};

fn round_trip(r: &Report) -> serde_json::Value {
    let text = serde_json::to_string(r).unwrap();
    let back: Report = serde_json::from_str(&text).unwrap();
    assert_eq!(&back, r);
    serde_json::from_str(&text).unwrap()
}

fn shell() -> Mesh {
    let mut m = sphere(3, 20.0, [0.0; 3]);
    let hole = sphere(3, 12.0, [0.0; 3]);
    let base = u32::try_from(m.vertices.len()).unwrap();
    m.vertices.extend(hole.vertices);
    m.triangles.extend(hole.triangles.iter().map(|[a, b, c]| [a + base, c + base, b + base]));
    m
}

#[test]
fn a_report_is_read_back_as_it_was_written() {
    let m = shell();
    let c = convert(&m, &Options::new(1.0)).unwrap();
    let points = [
        ("cavity".to_string(), [0.5, 0.5, 0.5]),
        ("wall".to_string(), [16.0, 0.1, -0.3]),
        ("outside".to_string(), [1e12, 0.0, 0.0]),
    ];
    let json = round_trip(&report(&m, &c, &points).unwrap());
    assert_eq!(json["solid"], serde_json::json!({"components": 1, "tunnels": 0, "cavities": 1}));
    assert_eq!(json["euler_change"], 0);
    assert_eq!(json["located"], true);
    assert_eq!(json["thin"], serde_json::json!([]));
    assert_eq!(json["reference"]["Closed"]["solid_euler"], 2);
    assert_eq!(json["regions"][1]["place"], "Solid");
    assert_eq!(json["regions"][2]["place"]["Complement"]["outside"], true);
    assert_eq!(json["apart"], serde_json::json!([[0, 2]]));

    // A torus: a tunnel.
    let ring = torus(20.0, 7.0, 32);
    let c = convert(&ring, &Options::new(1.0)).unwrap();
    let json = round_trip(&report(&ring, &c, &[]).unwrap());
    assert_eq!(json["solid"]["tunnels"], 1);
    assert_eq!(json["complement"]["tunnels"], 1);

    // An open input: no reference, the surface's counts instead, and no change.
    let mut open = sphere(3, 20.0, [0.0; 3]);
    open.triangles.pop();
    let c = convert(&open, &Options::new(1.0)).unwrap();
    let json = round_trip(&report(&open, &c, &[]).unwrap());
    assert_eq!(json["reference"]["None"]["surface"]["open_edges"], 3);
    assert!(json["euler_change"].is_null());
}
