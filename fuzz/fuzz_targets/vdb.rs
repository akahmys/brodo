//! Fuzzes `vdb::from_bytes`, the reader of OpenVDB files (`crates/brodo/src/vdb_read.rs`),
//! and a read of each grid it accepts: no input may panic (TESTING.md T6).

#![no_main]

use brodo::vdb::{VdbField, from_bytes};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(file) = from_bytes(data, "m") {
        for g in &file.grids {
            match &g.field {
                VdbField::F32(f) => {
                    let _value = f.value_at(&g.topology, [0, 0, 0]);
                }
                VdbField::F64(f) => {
                    let _value = f.value_at(&g.topology, [0, 0, 0]);
                }
                _ => {}
            }
        }
    }
});
