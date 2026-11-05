//! Fuzzes `colino::obj::from_bytes`, the reader of Wavefront OBJ files
//! (`crates/colino/src/obj.rs`), and welding what it accepts: no input may panic, and an
//! accepted mesh names only its own vertices, all finite (TESTING.md T6).

#![no_main]

use colino::mesh::Limits;
use colino::obj::from_bytes;
use libfuzzer_sys::fuzz_target;

const LIMITS: Limits = Limits::new(1 << 16, 1 << 18, 1 << 30);

fuzz_target!(|data: &[u8]| {
    if let Ok(mesh) = from_bytes(data, LIMITS) {
        let n = mesh.vertices.len();
        assert!(mesh.triangles.iter().flatten().all(|i| (*i as usize) < n));
        assert!(mesh.vertices.iter().flatten().all(|c| c.is_finite()));
        assert!(mesh.welded().is_ok());
    }
});
