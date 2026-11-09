//! Fuzzes `vdb::from_bytes` and `vdb::from_bytes_named`, the readers of OpenVDB files
//! (`crates/brodo/src/vdb_read.rs`), and reads of each grid they accept, at the shared
//! probes: no input may panic (TESTING.md T6), and a file read whole reads each of its names
//! as the grids of that name it read.
//! The limit keeps what a file may make within the fuzzer's memory (ADR-0047).

#![no_main]

use brodo::field::AnyField;
use brodo::vdb::{from_bytes, from_bytes_named};
use libfuzzer_sys::fuzz_target;

#[path = "shared/probes.rs"]
mod probes;
use probes::PROBES;

const LIMIT: u64 = 1 << 26;

fuzz_target!(|data: &[u8]| {
    let _unnamed = from_bytes_named(data, "m", "", LIMIT);
    if let Ok(file) = from_bytes(data, "m", LIMIT) {
        for g in &file.grids {
            let name = &g.meta.name;
            let named = from_bytes_named(data, "m", name, LIMIT).expect("a name read is read");
            let of_name = file.grids.iter().filter(|n| &n.meta.name == name).count();
            assert_eq!(named.grids.len(), of_name);
            assert!(named.skipped.iter().all(|s| s == name));
        }
        for g in &file.grids {
            match &g.field {
                AnyField::F32(f) => {
                    for v in PROBES {
                        let _value = f.value_at(&g.topology, v);
                    }
                }
                AnyField::F64(f) => {
                    for v in PROBES {
                        let _value = f.value_at(&g.topology, v);
                    }
                }
                _ => {}
            }
        }
    }
});
