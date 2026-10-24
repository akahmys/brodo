//! Format tier (TESTING.md §1) for OBJ: what is written reads back, through positive and
//! negative vertex numbers and every reference form; polygons become fans; what has no part
//! in the triangles is skipped and what would define shape otherwise, or reach outside the
//! file, is refused; each damage is its own error; damaged text is refused or read safely.

use colino::error::ReadError;
use colino::mesh::{Limits, Mesh};
use colino::obj::read;
use proptest::prelude::*;

const ROOMY: Limits = Limits::new(1 << 20, 1 << 20, 1 << 30);

fn read_str(s: &str) -> Result<Mesh, ReadError> {
    read(s.as_bytes(), ROOMY)
}

#[test]
fn the_specifications_cube_with_relative_numbers_reads() {
    // After the specification's example: four vertices, then a face naming them by -4..-1.
    let mut text = String::new();
    for z in [0, 1] {
        text.push_str(&format!("v 0 0 {z}\nv 1 0 {z}\nv 1 1 {z}\nv 0 1 {z}\nf -4 -3 -2 -1\n"));
    }
    let m = read_str(&text).unwrap();
    assert_eq!(m.vertices.len(), 8);
    assert_eq!(m.triangles, vec![[0, 1, 2], [0, 2, 3], [4, 5, 6], [4, 6, 7]]);
}

#[test]
fn a_polygon_becomes_a_fan_from_its_first_vertex() {
    let m = read_str("v 0 0 0\nv 1 0 0\nv 2 1 0\nv 1 2 0\nv 0 1 0\nf 1 2 3 4 5\n").unwrap();
    assert_eq!(m.triangles, vec![[0, 1, 2], [0, 2, 3], [0, 3, 4]]);
}

#[test]
fn reference_forms_comments_continuations_and_skipped_statements_read() {
    let text = "# a comment\nmtllib a.mtl\no thing\ng part\ns 1\nusemtl red\n\
                v 0 0 0 1.0\nv 1 0 0 0.5 0.5 0.5\nv 0 1 \\\n  0 # the z on the next line\n\
                vt 0 0\nvn 0 0 1\np 1\nl 1 2\n\
                f 1/1/1 2/1/1 3/1/1\nf 1//1 2//1 3//1\nf 1/1 2/1 3/1\nf 3 \\\n 2 1\n";
    let m = read_str(text).unwrap();
    assert_eq!(m.vertices.len(), 3);
    assert_eq!(m.vertices[2].map(f64::to_bits), [0.0, 1.0, 0.0].map(f64::to_bits));
    assert_eq!(m.triangles, vec![[0, 1, 2], [0, 1, 2], [0, 1, 2], [2, 1, 0]]);
    assert_eq!(m.unit, None);
}

/// Groups (ADR-0030): the last `g`'s names joined by a space, else the last `o`'s name,
/// else group 0; ids in the order a triangle first takes each name.
#[test]
fn faces_take_the_last_groups_names_else_the_objects() {
    let text = "v 0 0 0\nv 1 0 0\nv 0 1 0\nv 1 1 0\n\
                f 1 2 3\n\
                o body\nf 1 2 3 4\n\
                g left  right\nf 1 2 3\n\
                o other\nf 1 2 3\n\
                g\nf 1 2 3\n\
                g body\nf 1 2 3\n\
                g left right\nf 1 2 3\n";
    let m = read_str(text).unwrap();
    assert_eq!(m.groups, vec![0, 1, 1, 2, 2, 0, 1, 2]);
    assert_eq!(m.group_names, ["", "body", "left right"]);
    assert_eq!(m.check_groups(), Ok(()));
}

#[test]
fn a_file_without_groups_has_none() {
    let m = read_str("v 0 0 0\nv 1 0 0\nv 0 1 0\ng\nf 1 2 3\n").unwrap();
    assert!(m.groups.is_empty() && m.group_names.is_empty());
    let m = read_str("o \nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
    assert!(m.groups.is_empty() && m.group_names.is_empty());
}

#[test]
fn each_damage_is_its_own_error() {
    let three = "v 0 0 0\nv 1 0 0\nv 0 1 0\n";
    let cases: Vec<(String, ReadError)> = vec![
        (format!("{three}f 1 2 0\n"), ReadError::NoVertex { line: 4, index: 0 }),
        (format!("{three}f 1 2 4\n"), ReadError::NoVertex { line: 4, index: 4 }),
        (format!("{three}f 1 2 5\nf 1 2 5\n"), ReadError::NoVertex { line: 4, index: 5 }),
        (format!("{three}f -1 -2 -4\n"), ReadError::NoVertex { line: 4, index: -4 }),
        (format!("{three}f 1 2\n"), ReadError::BadFace { line: 4 }),
        (format!("{three}f 1 x 3\n"), ReadError::BadFace { line: 4 }),
        ("v 0 0\n".into(), ReadError::BadVertex { line: 1, numbers: 2 }),
        ("v 0 0 0 1 1\n".into(), ReadError::BadVertex { line: 1, numbers: 5 }),
        ("v 0 nan 0\n".into(), ReadError::BadNumberOnLine { line: 1, found: "nan".into() }),
        ("v 0 0 inf\n".into(), ReadError::BadNumberOnLine { line: 1, found: "inf".into() }),
        ("x 1 2 3\n".into(), ReadError::UnknownStatement { line: 1, word: "x".into() }),
        ("V 0 0 0\n".into(), ReadError::UnknownStatement { line: 1, word: "V".into() }),
        ("curv 0 1 1 2\n".into(), ReadError::Unsupported { line: 1, word: "curv".into() }),
        ("call other.obj\n".into(), ReadError::Unsupported { line: 1, word: "call".into() }),
        ("csh -rm x\n".into(), ReadError::Unsupported { line: 1, word: "csh".into() }),
    ];
    for (text, error) in cases {
        assert_eq!(read_str(&text), Err(error), "{text:?}");
    }
    // A number may name a vertex further on in the file.
    assert!(read_str("f 1 2 3\nv 0 0 0\nv 1 0 0\nv 0 1 0\n").is_ok());
    let limits = |triangles, vertices| Limits::new(triangles, vertices, 1 << 30);
    let quad = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3 4\n";
    assert_eq!(
        read(quad.as_bytes(), limits(1, 10)),
        Err(ReadError::TooLarge { what: "triangles", limit: 1 })
    );
    assert_eq!(
        read(quad.as_bytes(), limits(10, 3)),
        Err(ReadError::TooLarge { what: "vertices", limit: 3 })
    );
    assert_eq!(read(&[b'v', b' ', 0xff], ROOMY), Err(ReadError::NotText));
    let tiny = ROOMY.with_bytes(10);
    assert_eq!(read(quad.as_bytes(), tiny), Err(ReadError::TooLarge { what: "bytes", limit: 10 }));
}

fn coordinate() -> impl Strategy<Value = f64> {
    prop_oneof![Just(0.0), Just(-0.0), Just(1.5), Just(-2.0), Just(1e-9), Just(123456.789)]
}

/// A mesh's vertices and polygons, each polygon three to six vertex numbers from 0.
fn polygons() -> impl Strategy<Value = (Vec<[f64; 3]>, Vec<Vec<usize>>)> {
    prop::collection::vec([coordinate(), coordinate(), coordinate()], 3..20).prop_flat_map(|v| {
        let n = v.len();
        let polygon = prop::collection::vec(0..n, 3..7);
        (Just(v), prop::collection::vec(polygon, 0..15))
    })
}

/// The text: all vertices first, then each polygon by positive or negative numbers, in one
/// of the reference forms.
fn write(vertices: &[[f64; 3]], polygons: &[Vec<usize>], negative: bool, form: u8) -> String {
    let mut out = String::new();
    for [x, y, z] in vertices {
        out.push_str(&format!("v {x:?} {y:?} {z:?}\n"));
    }
    for p in polygons {
        out.push('f');
        for i in p {
            let n = if negative {
                format!("{}", i64::try_from(*i).unwrap() - i64::try_from(vertices.len()).unwrap())
            } else {
                format!("{}", i + 1)
            };
            out.push_str(&match form % 4 {
                0 => format!(" {n}"),
                1 => format!(" {n}/1"),
                2 => format!(" {n}/1/1"),
                _ => format!(" {n}//1"),
            });
        }
        out.push('\n');
    }
    out
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn written_polygons_read_back_as_fans(
        (vertices, polys) in polygons(),
        negative in any::<bool>(),
        form in any::<u8>(),
    ) {
        let m = read_str(&write(&vertices, &polys, negative, form)).unwrap();
        let bits = |v: &[[f64; 3]]| v.iter().flatten().map(|c| c.to_bits()).collect::<Vec<_>>();
        prop_assert_eq!(bits(&m.vertices), bits(&vertices));
        let fans: Vec<[u32; 3]> = polys
            .iter()
            .flat_map(|p| {
                let id = |i: usize| u32::try_from(p[i]).unwrap();
                (1..p.len() - 1).map(move |k| [id(0), id(k), id(k + 1)])
            })
            .collect();
        prop_assert_eq!(m.triangles, fans);
    }

    #[test]
    fn damaged_text_is_refused_or_read_safely(
        (vertices, polys) in polygons(),
        changes in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 1..6),
    ) {
        // Damaged with the characters OBJ text is made of, so that most reaches its words.
        const TEXT: &[u8] = b"0123456789.-+e /\\#\nvfntlgp";
        let mut bytes = write(&vertices, &polys, false, 0).into_bytes();
        for (at, b) in changes {
            let i = at.index(bytes.len());
            bytes[i] = TEXT[usize::from(b) % TEXT.len()];
        }
        if let Ok(m) = read(&bytes, ROOMY) {
            let n = m.vertices.len();
            prop_assert!(m.triangles.iter().flatten().all(|i| usize::try_from(*i).unwrap() < n));
            prop_assert!(m.vertices.iter().flatten().all(|c| c.is_finite()));
            prop_assert!(m.welded().is_ok());
        }
    }
}
