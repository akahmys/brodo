//! Format tier (TESTING.md §1) for STL: what is written reads back, binary or ASCII; what
//! the format descriptions leave open is read as `colino::stl` states; each damage is its
//! own error; damaged bytes are refused or read safely, never with a panic.

use colino::error::{MeshError, ReadError};
use colino::mesh::{Limits, Mesh};
use colino::stl::read;
use proptest::prelude::*;

const ROOMY: Limits = Limits { triangles: 1 << 20, vertices: 1 << 22, bytes: 1 << 30 };

/// A binary STL of triangles, with a header and, unused by readers, normals of zero.
fn binary(header: &[u8; 80], triangles: &[[[f32; 3]; 3]]) -> Vec<u8> {
    let mut out = header.to_vec();
    out.extend_from_slice(&u32::try_from(triangles.len()).unwrap().to_le_bytes());
    for t in triangles {
        out.extend_from_slice(&[0_u8; 12]);
        for c in t.iter().flatten() {
            out.extend_from_slice(&c.to_le_bytes());
        }
        out.extend_from_slice(&[0, 0]);
    }
    out
}

/// An ASCII STL of triangles, in one solid, numbers as Rust writes them.
fn ascii(triangles: &[[[f64; 3]; 3]]) -> String {
    let mut out = String::from("solid test\n");
    for t in triangles {
        out.push_str("  facet normal 0 0 1\n    outer loop\n");
        for [x, y, z] in t {
            out.push_str(&format!("      vertex {x} {y} {z}\n"));
        }
        out.push_str("    endloop\n  endfacet\n");
    }
    out.push_str("endsolid test\n");
    out
}

/// A tetrahedron's four faces, outward, corner by corner.
fn tetrahedron() -> Vec<[[f32; 3]; 3]> {
    let [a, b, c, d] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    vec![[a, c, b], [a, b, d], [a, d, c], [b, c, d]]
}

fn corners(m: &Mesh) -> Vec<[[f64; 3]; 3]> {
    m.triangles.iter().map(|t| t.map(|i| m.vertices[usize::try_from(i).unwrap()])).collect()
}

fn wide(t: &[[[f32; 3]; 3]]) -> Vec<[[f64; 3]; 3]> {
    t.iter().map(|t| t.map(|c| c.map(f64::from))).collect()
}

#[test]
fn a_tetrahedron_reads_back_and_welds_to_four_vertices() {
    let t = tetrahedron();
    for bytes in [binary(&[b' '; 80], &t), ascii(&wide(&t)).into_bytes()] {
        let m = read(&bytes, ROOMY).unwrap();
        assert_eq!(corners(&m), wide(&t));
        assert_eq!(m.vertices.len(), 12);
        assert_eq!(m.unit, None);
        let w = m.welded().unwrap();
        assert_eq!(w.vertices.len(), 4);
        assert_eq!(corners(&w), wide(&t), "welding moves nothing");
    }
}

#[test]
fn a_binary_file_beginning_with_solid_is_binary() {
    let mut header = [b' '; 80];
    header[..5].copy_from_slice(b"solid");
    let m = read(&binary(&header, &tetrahedron()), ROOMY).unwrap();
    assert_eq!(m.triangles.len(), 4);
}

#[test]
fn ascii_words_are_read_without_case_normals_optional_solids_several() {
    let text = "SOLID a b\n facet outer loop vertex 0 0 0 Vertex 1 0 0 VERTEX 0 1e0 0 endloop endfacet\n\
                EndSolid a b\nsolid\nfacet normal 0 0 -1 outer loop vertex 0 0 1 vertex 1 0 1 vertex 0 1 1 \
                endloop endfacet endsolid";
    let m = read(text.as_bytes(), ROOMY).unwrap();
    assert_eq!(m.triangles.len(), 2);
    assert_eq!(m.vertices[2].map(f64::to_bits), [0.0, 1.0, 0.0].map(f64::to_bits));
}

#[test]
fn each_damage_is_its_own_error() {
    let t = tetrahedron();
    let mut short = binary(&[b' '; 80], &t);
    short.pop();
    assert_eq!(
        read(&short, ROOMY),
        Err(ReadError::NotStl("neither the length of a binary file nor 'solid'"))
    );
    let mut not_finite = t.clone();
    not_finite[2][1][0] = f32::NAN;
    assert_eq!(read(&binary(&[b' '; 80], &not_finite), ROOMY), Err(ReadError::NotFinite(2)));
    assert_eq!(
        read(&binary(&[b' '; 80], &t), Limits { triangles: 3, vertices: 100, bytes: 1 << 30 }),
        Err(ReadError::TooLarge { what: "triangles", limit: 3 })
    );
    assert_eq!(
        read(&binary(&[b' '; 80], &t), Limits { triangles: 100, vertices: 11, bytes: 1 << 30 }),
        Err(ReadError::TooLarge { what: "vertices", limit: 11 })
    );
    // Exactly at each limit, the file reads.
    let file = binary(&[b' '; 80], &t);
    let exact = Limits { triangles: 4, vertices: 12, bytes: u64::try_from(file.len()).unwrap() };
    assert_eq!(read(&file, exact).map(|m| m.triangles.len()), Ok(4));
    let tiny = Limits { bytes: 83, ..ROOMY };
    assert_eq!(
        read(&binary(&[b' '; 80], &t), tiny),
        Err(ReadError::TooLarge { what: "bytes", limit: 83 })
    );
    let quad = "solid\nfacet outer loop vertex 0 0 0 vertex 1 0 0 vertex 1 1 0 vertex 0 1 0 endloop endfacet endsolid";
    assert_eq!(read(quad.as_bytes(), ROOMY), Err(ReadError::NotATriangle { at: 3, vertices: 4 }));
    for bad in ["nan", "inf", "1.0.0", "x"] {
        let text = format!(
            "solid facet outer loop vertex 0 0 {bad} vertex 1 0 0 vertex 0 1 0 endloop endfacet endsolid"
        );
        assert_eq!(
            read(text.as_bytes(), ROOMY),
            Err(ReadError::BadNumber { at: 7, found: bad.to_owned() })
        );
    }
    let cut = "solid facet outer loop vertex 0 0 0 vertex 1 0 0";
    assert!(matches!(
        read(cut.as_bytes(), ROOMY),
        Err(ReadError::Unexpected { expected: "endloop", .. })
    ));
    let after = "solid endsolid trailing words";
    assert!(read(after.as_bytes(), ROOMY).is_ok(), "a name may follow endsolid");
    // The words after endsolid, up to another solid, are its name, whatever they are.
    let named = "solid facet outer loop vertex 0 0 0 vertex 1 0 0 vertex 0 1 0 endloop endfacet endsolid x\nfacet";
    assert_eq!(read(named.as_bytes(), ROOMY).map(|m| m.triangles.len()), Ok(1));
    assert_eq!(read(&[b's', b'o', b'l', b'i', b'd', 0xff], ROOMY), Err(ReadError::NotText));
}

#[test]
fn welding_joins_zeros_of_either_sign_and_refuses_a_bad_index() {
    let m = Mesh {
        vertices: vec![[0.0, 1.0, 2.0], [-0.0, 1.0, 2.0], [3.0, 4.0, 5.0]],
        triangles: vec![[0, 1, 2]],
        unit: None,
    };
    let w = m.welded().unwrap();
    assert_eq!(w.vertices.len(), 2);
    assert_eq!(w.triangles, vec![[0, 0, 1]]);
    let bad = Mesh { triangles: vec![[0, 1, 3]], ..m };
    assert_eq!(bad.welded(), Err(MeshError::BadIndex { triangle: 0, index: 3 }));
}

/// Coordinates from a small set, zeros of both signs among them, so that welding finds
/// equal positions.
fn coordinate() -> impl Strategy<Value = f32> {
    prop_oneof![Just(0.0_f32), Just(-0.0), Just(1.0), Just(-2.5), Just(1e-7), Just(3.25e5)]
}

fn triangle() -> impl Strategy<Value = [[f32; 3]; 3]> {
    [
        [coordinate(), coordinate(), coordinate()],
        [coordinate(), coordinate(), coordinate()],
        [coordinate(), coordinate(), coordinate()],
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn written_triangles_read_back_and_weld_by_position(
        t in prop::collection::vec(triangle(), 0..40),
        as_text in any::<bool>(),
    ) {
        let bytes = if as_text { ascii(&wide(&t)).into_bytes() } else { binary(&[b' '; 80], &t) };
        let m = read(&bytes, ROOMY).unwrap();
        let got = corners(&m);
        // Zeros keep their sign through binary and ASCII alike: compare bits.
        let bits = |c: &[[[f64; 3]; 3]]| c.iter().flatten().flatten().map(|x| x.to_bits()).collect::<Vec<_>>();
        prop_assert_eq!(bits(&got), bits(&wide(&t)));
        let w = m.welded().unwrap();
        let distinct: std::collections::BTreeSet<[u64; 3]> =
            m.vertices.iter().map(|p| p.map(|c| (c + 0.0).to_bits())).collect();
        prop_assert_eq!(w.vertices.len(), distinct.len());
        prop_assert_eq!(w.triangles.len(), m.triangles.len());
    }

    #[test]
    fn damaged_bytes_are_refused_or_read_safely(
        t in prop::collection::vec(triangle(), 1..8),
        as_text in any::<bool>(),
        changes in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 1..6),
        cut in prop::option::of(any::<prop::sample::Index>()),
    ) {
        let mut bytes = if as_text { ascii(&wide(&t)).into_bytes() } else { binary(&[b' '; 80], &t) };
        // Text is damaged with the characters it is made of, so that most damage reaches its
        // words rather than stopping at UTF-8.
        const TEXT: &[u8] = b"0123456789.-+eE solidfacetnormaloutervrxp\n\t";
        for (at, b) in changes {
            let i = at.index(bytes.len());
            bytes[i] = if as_text { TEXT[usize::from(b) % TEXT.len()] } else { b };
        }
        let keep = cut.map_or(bytes.len(), |c| c.index(bytes.len() + 1));
        if let Ok(m) = read(&bytes[..keep], ROOMY) {
            let n = m.vertices.len();
            prop_assert!(m.triangles.iter().flatten().all(|i| usize::try_from(*i).unwrap() < n));
            prop_assert!(m.vertices.iter().flatten().all(|c| c.is_finite()));
            prop_assert!(m.welded().is_ok());
        }
    }
}

/// An ASCII file counts its triangles against the limit as it reads them.
#[test]
fn an_ascii_file_holds_to_the_triangle_limit() {
    let t: Vec<[[f64; 3]; 3]> = tetrahedron().iter().map(|f| f.map(|p| p.map(f64::from))).collect();
    let text = ascii(&t);
    let limits = |triangles| Limits { triangles, ..ROOMY };
    assert_eq!(read(text.as_bytes(), limits(4)).map(|m| m.triangles.len()), Ok(4));
    assert_eq!(
        read(text.as_bytes(), limits(3)),
        Err(ReadError::TooLarge { what: "triangles", limit: 3 })
    );
}
