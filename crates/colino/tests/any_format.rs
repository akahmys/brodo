//! Format tier (TESTING.md §1): a mesh file's format is told from its bytes by the rules of
//! `colino::any` (ADR-0055's amendment of 2026-11-17), and the reader of any format reads
//! each as its own reader does, at a path as from bytes, under the same limits (issue 10).

use std::path::PathBuf;

use colino::any::{Format, format_of, from_bytes, read};
use colino::error::ReadError;
use colino::mesh::{Limits, Mesh};

const LIMITS: Limits = Limits::new(1 << 16, 1 << 18, 1 << 30);

/// A binary STL file of one triangle, its 80-byte header beginning `header`.
fn binary_stl(header: &[u8]) -> Vec<u8> {
    let mut bytes = header.to_vec();
    bytes.resize(80, b' ');
    bytes.extend(1_u32.to_le_bytes());
    for c in [[0.0_f32; 3], [0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
        bytes.extend(c.iter().flat_map(|x| x.to_le_bytes()));
    }
    bytes.extend([0, 0]);
    bytes
}

const ASCII_STL: &str = "solid one
facet normal 0 0 1
outer loop
vertex 0 0 0
vertex 1 0 0
vertex 0 1 0
endloop
endfacet
endsolid one
";

const OBJ: &str = "# a triangle\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";

fn threemf() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/threemf_seed_stored.3mf");
    std::fs::read(path).unwrap()
}

fn same(a: &Mesh, b: &Mesh) -> bool {
    a.vertices == b.vertices && a.triangles == b.triangles && a.unit == b.unit
}

#[test]
fn each_format_is_told_from_its_bytes() {
    assert_eq!(format_of(&threemf()), Format::ThreeMf);
    assert_eq!(format_of(&binary_stl(b"made by a tool")), Format::Stl);
    // A binary file may begin `solid`: its length tells it.
    assert_eq!(format_of(&binary_stl(b"solid but binary")), Format::Stl);
    assert_eq!(format_of(ASCII_STL.as_bytes()), Format::Stl);
    assert_eq!(format_of(b" \r\n\tSOLID\n"), Format::Stl);
    assert_eq!(format_of(b"solid"), Format::Stl);
    assert_eq!(format_of(OBJ.as_bytes()), Format::Obj);
    // `solid` must be a word of its own; anything else is OBJ's to read or refuse.
    assert_eq!(format_of(b"solidity 1\n"), Format::Obj);
    assert_eq!(format_of(b"soli"), Format::Obj);
    assert_eq!(format_of(b""), Format::Obj);
    assert_eq!(format_of(b"PK\x03"), Format::Obj);
    // A binary STL's length one byte off is not binary.
    let mut long = binary_stl(b"x");
    long.push(0);
    assert_eq!(format_of(&long), Format::Obj);
}

#[test]
fn each_format_reads_as_its_own_reader_reads_it() {
    let cases: [(Vec<u8>, Mesh); 4] = [
        (
            binary_stl(b"solid but binary"),
            colino::stl::from_bytes(&binary_stl(b"solid but binary"), LIMITS).unwrap(),
        ),
        (ASCII_STL.into(), colino::stl::from_bytes(ASCII_STL.as_bytes(), LIMITS).unwrap()),
        (OBJ.into(), colino::obj::from_bytes(OBJ.as_bytes(), LIMITS).unwrap()),
        (threemf(), colino::threemf::from_bytes(&threemf(), LIMITS).unwrap()),
    ];
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    for (i, (bytes, expected)) in cases.iter().enumerate() {
        assert!(!expected.triangles.is_empty(), "case {i}");
        assert!(same(&from_bytes(bytes, LIMITS).unwrap(), expected), "case {i}");
        let path = dir.join(format!("any_format_case_{i}"));
        std::fs::write(&path, bytes).unwrap();
        assert!(same(&read(&path, LIMITS).unwrap(), expected), "case {i} at a path");
        let len = u64::try_from(bytes.len()).unwrap();
        for refused in
            [from_bytes(bytes, LIMITS.with_bytes(len - 1)), read(&path, LIMITS.with_bytes(len - 1))]
        {
            assert!(matches!(refused, Err(ReadError::TooLarge { what: "bytes", .. })), "case {i}");
        }
    }
    // What is none of the formats is refused by OBJ's reader, with its error.
    assert!(matches!(from_bytes(&[0xff, 0xfe], LIMITS), Err(ReadError::NotText)));
}

/// A file reporting no length, as a device does, is refused by the bytes read: no more
/// than the limit and one byte are read.
#[cfg(unix)]
#[test]
fn a_device_longer_than_the_limit_is_refused_by_the_bytes_read() {
    let zero = read(std::path::Path::new("/dev/zero"), LIMITS.with_bytes(1024));
    assert!(matches!(zero, Err(ReadError::TooLarge { what: "bytes", limit: 1024 })), "{zero:?}");
}
