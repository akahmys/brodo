//! Fuzzes `vdb::from_bytes`, the reader of OpenVDB files (`crates/brodo/src/vdb_read.rs`),
//! and reads of each grid it accepts, at the shared probes: no input may panic (TESTING.md T6).
//! The limit keeps what a file may make within the fuzzer's memory (ADR-0047).

#![no_main]

use brodo::vdb::{VdbField, from_bytes};
use libfuzzer_sys::fuzz_target;

#[path = "shared/probes.rs"]
mod probes;
use probes::PROBES;

const LIMIT: u64 = 1 << 26;

fuzz_target!(|data: &[u8]| {
    if let Ok(file) = from_bytes(data, "m", LIMIT) {
        for g in &file.grids {
            match &g.field {
                VdbField::F32(f) => {
                    for v in PROBES {
                        let _value = f.value_at(&g.topology, v);
                    }
                }
                VdbField::F64(f) => {
                    for v in PROBES {
                        let _value = f.value_at(&g.topology, v);
                    }
                }
                _ => {}
            }
        }
    }
});
