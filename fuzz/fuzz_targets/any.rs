//! Fuzzes `colino::any::from_bytes`, the reader of a mesh file of any format, told from its
//! bytes (`crates/colino/src/any.rs`), and welding what it accepts: no input may panic, an
//! accepted mesh names only its own vertices, all finite, and the format told is the one
//! whose reader gives the same answer (TESTING.md T6).

#![no_main]

use colino::any::{Format, format_of, from_bytes};
use colino::mesh::Limits;
use libfuzzer_sys::fuzz_target;

const LIMITS: Limits = Limits::new(1 << 16, 1 << 18, 1 << 30);

fuzz_target!(|data: &[u8]| {
    let read = from_bytes(data, LIMITS);
    let by_its_reader = match format_of(data) {
        Format::Stl => colino::stl::from_bytes(data, LIMITS),
        Format::Obj => colino::obj::from_bytes(data, LIMITS),
        Format::ThreeMf => colino::threemf::from_bytes(data, LIMITS),
        _ => unreachable!("a format colino does not read"),
    };
    assert_eq!(read.is_ok(), by_its_reader.is_ok());
    if let Ok(mesh) = read {
        let n = mesh.vertices.len();
        assert!(mesh.triangles.iter().flatten().all(|i| (*i as usize) < n));
        assert!(mesh.vertices.iter().flatten().all(|c| c.is_finite()));
        assert!(mesh.welded().is_ok());
    }
});
