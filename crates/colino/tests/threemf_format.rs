//! Format tier (TESTING.md §1) for 3MF: packages written here, stored or deflated, read
//! back as the 3MF Core Specification 1.4.0 places their objects; each break of the
//! package, the XML or the model is its own error; damaged packages are refused or read
//! safely. Packages are written by a small ZIP writer after PKWARE's APPNOTE.

use colino::error::ReadError;
use colino::mesh::{Limits, Mesh};
use colino::threemf::read;
use proptest::prelude::*;

const ROOMY: Limits = Limits::new(1 << 20, 1 << 20, 1 << 26);
const CORE: &str = "http://schemas.microsoft.com/3dmanufacturing/core/2015/02";
const RELS: &str = "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
    <Relationship Target=\"/3D/3dmodel.model\" Id=\"rel0\" \
    Type=\"http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel\"/></Relationships>";
const TYPES: &str =
    "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"/>";

/// One part of a package, and how to write it, wrongly too.
struct Part<'a> {
    name: &'a str,
    data: Vec<u8>,
    method: u16,
    crc_off: u32,
    size_off: u32,
}

fn stored<'a>(name: &'a str, data: &[u8]) -> Part<'a> {
    Part { name, data: data.to_vec(), method: 0, crc_off: 0, size_off: 0 }
}

fn deflated<'a>(name: &'a str, data: &[u8]) -> Part<'a> {
    Part { name, data: data.to_vec(), method: 8, crc_off: 0, size_off: 0 }
}

/// A ZIP archive of the parts: local headers and data, the central directory, its end.
fn zip(parts: &[Part<'_>]) -> Vec<u8> {
    let (mut out, mut directory) = (Vec::new(), Vec::new());
    let word = |v: &mut Vec<u8>, x: u32| v.extend_from_slice(&x.to_le_bytes());
    let half = |v: &mut Vec<u8>, x: u16| v.extend_from_slice(&x.to_le_bytes());
    for p in parts {
        let data = if p.method == 8 {
            miniz_oxide::deflate::compress_to_vec(&p.data, 6)
        } else {
            p.data.clone()
        };
        let crc = rawzip::crc32(&p.data) ^ p.crc_off;
        let size = u32::try_from(p.data.len()).unwrap() + p.size_off;
        let offset = u32::try_from(out.len()).unwrap();
        let name = p.name.as_bytes();
        for (v, signature) in [(&mut out, 0x0403_4b50_u32), (&mut directory, 0x0201_4b50)] {
            word(v, signature);
            if signature == 0x0201_4b50 {
                half(v, 20);
            }
            for x in [20, 0, p.method, 0, 0] {
                half(v, x);
            }
            for x in [crc, u32::try_from(data.len()).unwrap(), size] {
                word(v, x);
            }
            half(v, u16::try_from(name.len()).unwrap());
            half(v, 0);
            if signature == 0x0201_4b50 {
                for x in [0, 0, 0] {
                    half(v, x);
                }
                word(v, 0);
                word(v, offset);
            }
            v.extend_from_slice(name);
        }
        out.extend_from_slice(&data);
    }
    let start = u32::try_from(out.len()).unwrap();
    let count = u16::try_from(parts.len()).unwrap();
    out.extend_from_slice(&directory);
    word(&mut out, 0x0605_4b50);
    for x in [0, 0, count, count] {
        half(&mut out, x);
    }
    word(&mut out, u32::try_from(directory.len()).unwrap());
    word(&mut out, start);
    half(&mut out, 0);
    out
}

/// A model part around `body`, with attributes `attrs` on the model.
fn model(attrs: &str, body: &str) -> Vec<u8> {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<model xmlns=\"{CORE}\" {attrs}>{body}</model>"
    )
    .into_bytes()
}

/// A package of the relationships, content types and a model part, all stored.
fn package(model: Vec<u8>) -> Vec<u8> {
    zip(&[
        stored("[Content_Types].xml", TYPES.as_bytes()),
        stored("_rels/.rels", RELS.as_bytes()),
        Part { name: "3D/3dmodel.model", data: model, method: 0, crc_off: 0, size_off: 0 },
    ])
}

fn mesh_object(id: u32, kind: &str, v: &[[f64; 3]], t: &[[u32; 3]]) -> String {
    let mut s = format!("<object id=\"{id}\" type=\"{kind}\"><mesh><vertices>");
    for [x, y, z] in v {
        s.push_str(&format!("<vertex x=\"{x:?}\" y=\"{y:?}\" z=\"{z:?}\"/>"));
    }
    s.push_str("</vertices><triangles>");
    for [a, b, c] in t {
        s.push_str(&format!("<triangle v1=\"{a}\" v2=\"{b}\" v3=\"{c}\"/>"));
    }
    s.push_str("</triangles></mesh></object>");
    s
}

fn tetra() -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
    (
        vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
    )
}

fn one(build: &str, resources: &str) -> Result<Mesh, ReadError> {
    read(
        &package(model("", &format!("<resources>{resources}</resources><build>{build}</build>"))),
        ROOMY,
    )
}

fn bits(v: &[[f64; 3]]) -> Vec<u64> {
    v.iter().flatten().map(|c| c.to_bits()).collect()
}

#[test]
fn a_tetrahedron_reads_stored_and_deflated() {
    let (v, t) = tetra();
    let body = format!(
        "<resources>{}</resources><build><item objectid=\"1\"/></build>",
        mesh_object(1, "model", &v, &t)
    );
    let m = model("", &body);
    let pack = |deflate: bool| {
        let rels = if deflate {
            deflated("_rels/.rels", RELS.as_bytes())
        } else {
            stored("_rels/.rels", RELS.as_bytes())
        };
        let part =
            if deflate { deflated("3D/3dmodel.model", &m) } else { stored("3D/3dmodel.model", &m) };
        zip(&[rels, part])
    };
    for deflate in [false, true] {
        let got = read(&pack(deflate), ROOMY).unwrap();
        assert_eq!(bits(&got.vertices), bits(&v));
        assert_eq!(got.triangles, t);
        assert_eq!(got.unit.as_deref(), Some("millimeter"));
    }
}

#[test]
fn components_move_first_then_the_item_and_a_mirror_turns_triangles_over() {
    let (v, t) = tetra();
    let resources = format!(
        "{}<object id=\"2\"><components><component objectid=\"1\" transform=\"1 0 0 0 1 0 0 0 1 10 0 0\"/></components></object>",
        mesh_object(1, "model", &v, &t)
    );
    let got =
        one("<item objectid=\"2\" transform=\"2 0 0 0 2 0 0 0 2 0 0 0\"/>", &resources).unwrap();
    let want: Vec<[f64; 3]> =
        v.iter().map(|p| [(p[0] + 10.0) * 2.0, p[1] * 2.0, p[2] * 2.0]).collect();
    assert_eq!(bits(&got.vertices), bits(&want), "(v + 10) × 2, not 2v + 10");
    assert_eq!(got.triangles, t);
    let mirrored =
        one("<item objectid=\"1\" transform=\"-1 0 0 0 1 0 0 0 1 0 0 0\"/>", &resources).unwrap();
    assert_eq!(mirrored.triangles, t.iter().map(|[a, b, c]| [*a, *c, *b]).collect::<Vec<_>>());
}

/// Groups (ADR-0030): each object holding a mesh is a group, named by its `name`, else by
/// its `id` as written; every placement of it, through components or items, shares it.
#[test]
fn each_object_is_a_group_named_by_its_name_else_its_id() {
    let (v, t) = tetra();
    let triangle = mesh_object(0, "model", &v[..3], &[[0, 1, 2]]);
    let resources = format!(
        "{}{}{}<object id=\"4\" name=\"Assembly\"><components>\
         <component objectid=\"1\"/><component objectid=\"1\" transform=\"1 0 0 0 1 0 0 0 1 5 0 0\"/>\
         <component objectid=\"7\"/></components></object>",
        mesh_object(1, "model", &v, &t).replacen("id=\"1\"", "id=\"1\" name=\"Bolt\"", 1),
        triangle.replacen("id=\"0\"", "id=\"07\"", 1),
        triangle.replacen("id=\"0\"", "id=\"3\" name=\"\"", 1),
    );
    let m = one("<item objectid=\"4\"/><item objectid=\"7\"/><item objectid=\"3\"/>", &resources)
        .unwrap();
    assert_eq!(m.triangles.len(), 11);
    assert_eq!(m.groups, vec![1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 3]);
    assert_eq!(m.group_names, ["", "Bolt", "07", "3"]);
    assert_eq!(m.check_groups(), Ok(()));
}

#[test]
fn units_types_and_extensions_are_read_as_the_format_says() {
    let (v, t) = tetra();
    let tet = |id, kind| mesh_object(id, kind, &v, &t);
    let inch = read(
        &package(model(
            "unit=\"inch\"",
            &format!(
                "<resources>{}</resources><build><item objectid=\"1\"/></build>",
                tet(1, "model")
            ),
        )),
        ROOMY,
    )
    .unwrap();
    assert_eq!(inch.unit.as_deref(), Some("inch"));
    let bad_unit = read(&package(model("unit=\"furlong\"", "<resources/><build/>")), ROOMY);
    assert!(matches!(
        bad_unit,
        Err(ReadError::Model { reason: "a unit the format does not name", .. })
    ));
    // An object no item reaches is not returned; a support is skipped.
    let resources = format!("{}{}{}", tet(1, "model"), tet(2, "model"), tet(3, "support"));
    let got = one("<item objectid=\"1\"/><item objectid=\"3\"/>", &resources).unwrap();
    assert_eq!(got.triangles.len(), 4);
    for kind in ["surface", "other"] {
        let r = one("<item objectid=\"1\"/>", &tet(1, kind));
        assert!(
            matches!(
                r,
                Err(ReadError::Model { reason: "a surface or other object in the build", .. })
            ),
            "{kind}"
        );
    }
    let required = read(
        &package(model("xmlns:p=\"urn:p\" requiredextensions=\"p\"", "<resources/><build/>")),
        ROOMY,
    );
    assert_eq!(required, Err(ReadError::RequiredExtension("p".into())));
    // Extension elements and prefixed attributes, not required, are skipped.
    let ext = format!(
        "<resources><x:thing xmlns:x=\"urn:x\"><vertex x=\"9\"/></x:thing>{}</resources><build><item x:id=\"7\" xmlns:x=\"urn:x\" objectid=\"1\"/></build>",
        tet(1, "model")
    );
    assert_eq!(read(&package(model("", &ext)), ROOMY).unwrap().triangles.len(), 4);
}

#[test]
fn each_break_of_the_xml_is_its_own_error() {
    let model_error = |text: &str| match read(&package(text.as_bytes().to_vec()), ROOMY) {
        Err(ReadError::Model { reason, .. }) => reason.to_owned(),
        other => format!("{other:?}"),
    };
    let m = |body: &str| format!("<model xmlns=\"{CORE}\">{body}</model>");
    assert_eq!(
        model_error(&format!("<!DOCTYPE m [<!ENTITY e \"x\">]>{}", m(""))),
        "a document type declaration"
    );
    assert_eq!(model_error(&m("<p:a/>")), "an undeclared namespace prefix");
    assert_eq!(
        model_error(&m("<metadata name=\"a\">&custom;</metadata>")),
        "an entity other than XML's predefined ones"
    );
    assert_eq!(model_error(&format!("{}{}", m(""), m(""))), "a second root element");
    assert_eq!(model_error(&format!("<model xmlns=\"{CORE}\">")), "an element left open");
    assert_eq!(model_error(&format!("{} x", m(""))), "text outside the root element");
    assert_eq!(model_error("<model/>"), "an element in no namespace");
    assert!(matches!(
        read(&package(m("<metadata name=\"a\" name=\"b\"/>").into_bytes()), ROOMY),
        Err(ReadError::Xml { .. })
    ));
    assert!(matches!(
        read(&package(m("<resources></build>").into_bytes()), ROOMY),
        Err(ReadError::Xml { .. })
    ));
}

#[test]
fn each_break_of_the_model_is_its_own_error() {
    let model_error = |text: &str| match read(&package(text.as_bytes().to_vec()), ROOMY) {
        Err(ReadError::Model { reason, .. }) => reason.to_owned(),
        other => format!("{other:?}"),
    };
    let m = |body: &str| format!("<model xmlns=\"{CORE}\">{body}</model>");
    assert_eq!(
        model_error(&m("<resources><vertex x=\"0\" y=\"0\" z=\"0\"/></resources>")),
        "an element where the format puts none"
    );
    assert_eq!(
        model_error(&m("<resources><object id=\"1\"/></resources>")),
        "an object with neither mesh nor components"
    );
    assert_eq!(
        model_error(&m(
            "<resources><object id=\"1\"><mesh><vertices/><triangles><triangle v1=\"0\" v2=\"1\" v3=\"2\"/></triangles></mesh></object></resources>"
        )),
        "a triangle naming a vertex the mesh lacks"
    );
    assert_eq!(
        model_error(&m(
            "<resources><object id=\"1\"><mesh><vertices><vertex x=\"0\" y=\"inf\" z=\"0\"/></vertices></mesh></object></resources>"
        )),
        "a coordinate missing or not a finite number"
    );
    assert_eq!(
        model_error(&m(&format!(
            "<resources>{}</resources><build><item objectid=\"1\" transform=\"1 0 0\"/></build>",
            mesh_object(1, "model", &tetra().0, &tetra().1)
        ))),
        "a transform that is not 12 finite numbers"
    );
    assert!(
        matches!(read(&package(m("<widget/>").into_bytes()), ROOMY), Err(ReadError::UnknownElement { name, .. }) if name == "widget")
    );
    assert!(matches!(
        read(&package(m("<build><item objectid=\"4\"/></build>").into_bytes()), ROOMY),
        Err(ReadError::NoObject { id: 4, .. })
    ));
    let later = m(
        "<resources><object id=\"1\"><components><component objectid=\"2\"/></components></object></resources>",
    );
    assert!(
        matches!(read(&package(later.into_bytes()), ROOMY), Err(ReadError::NoObject { id: 2, .. })),
        "an object defined after"
    );
}

#[test]
fn a_long_name_placed_many_times_is_looked_up_once_in_time() {
    // A triangle named by 16 MiB, placed 2¹⁸ times through objects of two components each:
    // looked up once a placement, the name would be compared for 4 TiB.
    let (v, _) = tetra();
    let name = "n".repeat(1 << 24);
    let mut resources = mesh_object(1, "model", &v[..3], &[[0, 1, 2]]).replacen(
        "id=\"1\"",
        &format!("id=\"1\" name=\"{name}\""),
        1,
    );
    for id in 2..=19 {
        let below = id - 1;
        resources.push_str(&format!(
            "<object id=\"{id}\"><components><component objectid=\"{below}\"/><component objectid=\"{below}\"/></components></object>"
        ));
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        tx.send(one("<item objectid=\"19\"/>", &resources)).unwrap_or(());
    });
    let m = rx.recv_timeout(std::time::Duration::from_secs(60)).unwrap().unwrap();
    assert_eq!((m.triangles.len(), m.groups.len()), (1 << 18, 1 << 18));
    assert_eq!(m.group_names.len(), 2);
}

#[test]
fn an_object_naming_itself_as_a_component_is_refused_in_time() {
    // Its id is known from its own element's start, so its component would resolve to the
    // object still being read: expanded, it never ends (once named) or doubles on every
    // pass (twice). Such a component names no object defined before it.
    let m = |body: &str| format!("<model xmlns=\"{CORE}\">{body}</model>");
    for components in
        ["<component objectid=\"1\"/>", "<component objectid=\"1\"/><component objectid=\"1\"/>"]
    {
        let text = m(&format!(
            "<resources><object id=\"1\"><components>{components}</components></object></resources><build><item objectid=\"1\"/></build>"
        ));
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            tx.send(read(&package(text.into_bytes()), ROOMY)).unwrap_or(());
        });
        let read = rx.recv_timeout(std::time::Duration::from_secs(60));
        assert!(
            matches!(read, Ok(Err(ReadError::NoObject { id: 1, .. }))),
            "{components}: {read:?}"
        );
    }
}

#[test]
fn each_break_of_the_package_is_its_own_error() {
    let m = model("", "<resources/><build/>");
    let rels = |target: &str| RELS.replace("/3D/3dmodel.model", target);
    let error = |parts: &[Part<'_>]| read(&zip(parts), ROOMY);
    assert_eq!(read(b"not a zip", ROOMY), Err(ReadError::Package("not a ZIP archive")));
    assert_eq!(
        error(&[stored("3D/3dmodel.model", &m)]),
        Err(ReadError::Package("a part the package names is missing"))
    );
    assert_eq!(
        error(&[stored("_rels/.rels", rels("/elsewhere.model").as_bytes())]),
        Err(ReadError::Package("a part the package names is missing"))
    );
    assert_eq!(
        error(&[stored("_rels/.rels", rels("/../x").as_bytes())]),
        Err(ReadError::Package("a start part's name that is not a plain path"))
    );
    let two = RELS.replace("</Relationships>", "<Relationship Target=\"/a\" Id=\"r1\" Type=\"http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel\"/></Relationships>");
    assert_eq!(
        error(&[stored("_rels/.rels", two.as_bytes())]),
        Err(ReadError::Package("two start parts"))
    );
    let none =
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"/>";
    assert_eq!(
        error(&[stored("_rels/.rels", none.as_bytes())]),
        Err(ReadError::Package("no start part"))
    );
    let twice = [
        stored("_rels/.rels", RELS.as_bytes()),
        stored("3D/3dmodel.model", &m),
        stored("3D/3DModel.model", &m),
    ];
    assert_eq!(error(&twice), Err(ReadError::Package("two parts of one name")));
    let rels_part = || stored("_rels/.rels", RELS.as_bytes());
    let bad_crc = Part { crc_off: 1, ..stored("3D/3dmodel.model", &m) };
    assert_eq!(
        error(&[rels_part(), bad_crc]),
        Err(ReadError::Package("a part whose CRC does not match"))
    );
    let long = Part { size_off: 1, ..deflated("3D/3dmodel.model", &m) };
    assert_eq!(
        error(&[rels_part(), long]),
        Err(ReadError::Package("a part that does not expand to its size"))
    );
    let bzip = Part { method: 12, ..stored("3D/3dmodel.model", &m) };
    assert!(matches!(error(&[rels_part(), bzip]), Err(ReadError::Compression(_))));
}

#[test]
fn a_deflated_bomb_stops_at_the_byte_limit() {
    let m = model("", "<resources/><build/>");
    let rels_part = || stored("_rels/.rels", RELS.as_bytes());
    // A deflated bomb: 32 MiB of spaces from a few kilobytes, past a limit of 1 MiB.
    let mut bomb = m.clone();
    bomb.extend(std::iter::repeat_n(b' ', 32 << 20));
    let small = ROOMY.with_bytes(1 << 20);
    let packed = zip(&[rels_part(), deflated("3D/3dmodel.model", &bomb)]);
    assert!(packed.len() < 64 << 10);
    assert_eq!(read(&packed, small), Err(ReadError::TooLarge { what: "bytes", limit: 1 << 20 }));
}

#[test]
fn limits_hold_before_placements_are_expanded() {
    let (v, t) = tetra();
    // Forty objects, each placing the one before twice: 2^40 placements.
    let mut resources = mesh_object(1, "model", &v, &t);
    for id in 2..=40 {
        resources.push_str(&format!("<object id=\"{id}\"><components><component objectid=\"{}\"/><component objectid=\"{}\"/></components></object>", id - 1, id - 1));
    }
    let r = one("<item objectid=\"40\"/>", &resources);
    assert!(matches!(r, Err(ReadError::TooLarge { .. })), "{r:?}");
    // The same of an empty mesh: no vertex or triangle to count, only placements.
    let mut empty = String::from("<object id=\"1\"><mesh><vertices/><triangles/></mesh></object>");
    for id in 2..=40 {
        empty.push_str(&format!("<object id=\"{id}\"><components><component objectid=\"{}\"/><component objectid=\"{}\"/></components></object>", id - 1, id - 1));
    }
    let r = one("<item objectid=\"40\"/>", &empty);
    assert_eq!(r, Err(ReadError::TooLarge { what: "triangles", limit: 1 << 20 }));
    let few = ROOMY.with_vertices(3);
    let r = read(
        &package(model(
            "",
            &format!("<resources>{}</resources><build/>", mesh_object(1, "model", &v, &t)),
        )),
        few,
    );
    assert_eq!(r, Err(ReadError::TooLarge { what: "vertices", limit: 3 }));
}

// ---- Generated models, against a reference that places step by step.

/// Exact transforms: axis permutations with signs, scales by powers of two, integer moves.
fn transform() -> impl Strategy<Value = [f64; 12]> {
    let scale = || prop_oneof![Just(1.0), Just(-1.0), Just(2.0), Just(0.5)];
    (0..6_usize, [scale(), scale(), scale()], [-8..8_i32, -8..8, -8..8]).prop_map(|(perm, s, t)| {
        let order = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]][perm];
        let mut m = [0.0; 12];
        for (row, col) in order.iter().enumerate() {
            m[row * 3 + col] = s[row];
        }
        for k in 0..3 {
            m[9 + k] = f64::from(t[k]);
        }
        m
    })
}

fn apply(m: &[f64; 12], p: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|c| p[0] * m[c] + p[1] * m[3 + c] + p[2] * m[6 + c] + m[9 + c])
}

fn det(m: &[f64; 12]) -> f64 {
    m[0] * (m[4] * m[8] - m[5] * m[7]) - m[1] * (m[3] * m[8] - m[5] * m[6])
        + m[2] * (m[3] * m[7] - m[4] * m[6])
}

#[derive(Debug, Clone)]
enum Generated {
    Mesh(Vec<[f64; 3]>, Vec<[u32; 3]>),
    Components(Vec<(usize, [f64; 12])>),
}

/// Objects, each a mesh or components of objects before it, and build items.
fn models() -> impl Strategy<Value = (Vec<Generated>, Vec<(usize, [f64; 12])>)> {
    let mesh = (1..6_usize)
        .prop_flat_map(|n| {
            (
                prop::collection::vec([-4..4_i32, -4..4, -4..4], n),
                prop::collection::vec([0..n, 0..n, 0..n], 0..6),
            )
        })
        .prop_map(|(v, t)| {
            Generated::Mesh(
                v.into_iter().map(|p| p.map(|c| f64::from(c) / 2.0)).collect(),
                t.into_iter().map(|t| t.map(|i| u32::try_from(i).unwrap())).collect(),
            )
        });
    (
        mesh.clone(),
        prop::collection::vec(
            (
                any::<bool>(),
                mesh,
                prop::collection::vec((any::<prop::sample::Index>(), transform()), 1..3),
            ),
            0..5,
        ),
        prop::collection::vec((any::<prop::sample::Index>(), transform()), 1..4),
    )
        .prop_map(|(first, rest, items)| {
            let mut objects = vec![first];
            for (is_mesh, m, comps) in rest {
                let n = objects.len();
                objects.push(if is_mesh {
                    m
                } else {
                    Generated::Components(comps.into_iter().map(|(i, t)| (i.index(n), t)).collect())
                });
            }
            let n = objects.len();
            (objects, items.into_iter().map(|(i, t)| (i.index(n), t)).collect())
        })
}

fn write_model(objects: &[Generated], items: &[(usize, [f64; 12])]) -> Vec<u8> {
    let words = |m: &[f64; 12]| m.iter().map(|x| format!("{x:?}")).collect::<Vec<_>>().join(" ");
    let mut body = String::from("<resources>");
    for (i, o) in objects.iter().enumerate() {
        match o {
            Generated::Mesh(v, t) => {
                body.push_str(&mesh_object(u32::try_from(i + 1).unwrap(), "model", v, t))
            }
            Generated::Components(c) => {
                body.push_str(&format!("<object id=\"{}\"><components>", i + 1));
                for (child, m) in c {
                    body.push_str(&format!(
                        "<component objectid=\"{}\" transform=\"{}\"/>",
                        child + 1,
                        words(m)
                    ));
                }
                body.push_str("</components></object>");
            }
        }
    }
    body.push_str("</resources><build>");
    for (i, m) in items {
        body.push_str(&format!("<item objectid=\"{}\" transform=\"{}\"/>", i + 1, words(m)));
    }
    body.push_str("</build>");
    model("", &body)
}

/// The reference: places an object by applying its transforms one at a time, innermost
/// first.
fn place(objects: &[Generated], i: usize, moves: &[[f64; 12]], out: &mut Mesh) {
    match &objects[i] {
        Generated::Mesh(v, t) => {
            let base = u32::try_from(out.vertices.len()).unwrap();
            out.vertices.extend(v.iter().map(|p| moves.iter().fold(*p, |q, m| apply(m, q))));
            let flip = moves.iter().filter(|m| det(m) < 0.0).count() % 2 == 1;
            out.triangles.extend(t.iter().map(|[a, b, c]| {
                if flip { [base + a, base + c, base + b] } else { [base + a, base + b, base + c] }
            }));
        }
        Generated::Components(c) => {
            for (child, m) in c {
                let mut inner = vec![*m];
                inner.extend_from_slice(moves);
                place(objects, *child, &inner, out);
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn generated_models_place_as_step_by_step_placement_does((objects, items) in models(), deflate in any::<bool>()) {
        let m = write_model(&objects, &items);
        let part = if deflate { deflated("3D/3dmodel.model", &m) } else { stored("3D/3dmodel.model", &m) };
        let got = read(&zip(&[stored("_rels/.rels", RELS.as_bytes()), part]), ROOMY).unwrap();
        let mut want = Mesh::default();
        for (i, t) in &items {
            place(&objects, *i, &[*t], &mut want);
        }
        prop_assert_eq!(bits(&got.vertices), bits(&want.vertices));
        prop_assert_eq!(got.triangles, want.triangles);
    }

    #[test]
    fn damaged_packages_are_refused_or_read_safely(
        (objects, items) in models(),
        deflate in any::<bool>(),
        changes in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 1..6),
        in_the_xml in any::<bool>(),
    ) {
        // The package's bytes, which its CRCs mostly catch; or the model's XML before it is
        // packed, with the characters XML is made of, so that damage reaches the parser.
        const XML: &[u8] = b"<>/=\" \n0123456789.-eE:xmlnsobjectvertextrianglecomponentitembuild&;#!";
        let mut m = write_model(&objects, &items);
        if in_the_xml {
            for (at, b) in &changes {
                let i = at.index(m.len());
                m[i] = XML[usize::from(*b) % XML.len()];
            }
        }
        let part = if deflate { deflated("3D/3dmodel.model", &m) } else { stored("3D/3dmodel.model", &m) };
        let mut bytes = zip(&[stored("_rels/.rels", RELS.as_bytes()), part]);
        if !in_the_xml {
            for (at, b) in changes {
                let i = at.index(bytes.len());
                bytes[i] = b;
            }
        }
        if let Ok(mesh) = read(&bytes, ROOMY) {
            let n = mesh.vertices.len();
            prop_assert!(mesh.triangles.iter().flatten().all(|i| usize::try_from(*i).unwrap() < n));
            prop_assert!(mesh.vertices.iter().flatten().all(|c| c.is_finite()));
        }
    }
}

/// What a 3MF writer may put, which a strict reading must still take: a thumbnail's
/// relationship beside the model's, the predefined and character references, CDATA in an
/// element, foreign elements nested in one another, a flattening transform.
#[test]
fn what_the_format_allows_is_read() {
    let (v, t) = tetra();
    let body = format!(
        "<metadata name=\"Title\">a &amp; b &#65;<![CDATA[<c>]]></metadata>\
         <p:x><p:y><p:z/></p:y></p:x>\
         <resources>{}</resources><build><item objectid=\"1\"/></build>",
        mesh_object(1, "model", &v, &t)
    );
    let m = format!(
        "<?xml version=\"1.0\"?>\n<model xmlns=\"{CORE}\" xmlns:p=\"urn:x\">{body}</model>"
    );
    let thumbnail = RELS.replace(
        "<Relationship ",
        "<Relationship Target=\"/Metadata/thumbnail.png\" Id=\"t0\" \
         Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/thumbnail\"/>\
         <Relationship ",
    );
    let parts = [
        stored("[Content_Types].xml", TYPES.as_bytes()),
        stored("_rels/.rels", thumbnail.as_bytes()),
        stored("3D/3dmodel.model", m.as_bytes()),
    ];
    let read_back = read(&zip(&parts), ROOMY).map(|m| (bits(&m.vertices), m.triangles));
    assert_eq!(read_back, Ok((bits(&v), t.clone())));
    // A transform that flattens z: its determinant is zero, so nothing turns over.
    let flat = one(
        "<item objectid=\"1\" transform=\"1 0 0 0 1 0 0 0 0 0 0 0\"/>",
        &mesh_object(1, "model", &v, &t),
    )
    .unwrap();
    assert_eq!(flat.triangles, t);
}

/// Each break the format forbids that the tests above leave out.
#[test]
fn what_the_format_forbids_is_refused() {
    let model_error = |text: &str| match read(&package(text.as_bytes().to_vec()), ROOMY) {
        Err(ReadError::Model { at, reason }) => Ok((at, reason.to_owned())),
        other => Err(format!("{other:?}")),
    };
    let reason = |text: &str| model_error(text).map(|(_, r)| r);
    let m = |body: &str| format!("<model xmlns=\"{CORE}\">{body}</model>");
    assert_eq!(reason("<?xml version=\"1.0\"?>\n"), Ok("no root element".to_owned()));
    assert_eq!(
        reason(&format!("{}<![CDATA[x]]>", m(""))),
        Ok("text outside the root element".to_owned())
    );
    // An object with neither mesh nor components is refused where it ends.
    let text = m("<resources><object id=\"1\"></object><object id=\"2\"/></resources>");
    let (at, why) = model_error(&text).unwrap();
    assert_eq!(why, "an object with neither mesh nor components");
    assert!(at < u64::try_from(text.find("<object id=\"2\"").unwrap()).unwrap(), "{at}");
    let rels = |target: &str| RELS.replace("/3D/3dmodel.model", target);
    assert_eq!(
        read(&zip(&[stored("_rels/.rels", rels("/3D//3dmodel.model").as_bytes())]), ROOMY),
        Err(ReadError::Package("a start part's name that is not a plain path"))
    );
}

/// Limits hold at their exact values: as many vertices, triangles and placements as the
/// limits allow are read; one more, anywhere in the model, is refused.
#[test]
fn limits_hold_at_their_exact_values() {
    let (v, t) = tetra();
    let tetra_model = |extra: &str| {
        package(model(
            "",
            &format!(
                "<resources>{}{extra}</resources><build><item objectid=\"1\"/></build>",
                mesh_object(1, "model", &v, &t)
            ),
        ))
    };
    let exact = ROOMY.with_triangles(4).with_vertices(4);
    assert_eq!(read(&tetra_model(""), exact).map(|m| m.triangles.len()), Ok(4));
    // A second object, never placed, still counts as read.
    let unused = mesh_object(2, "model", &v, &t);
    let six = ROOMY.with_triangles(6).with_vertices(8);
    assert_eq!(
        read(&tetra_model(&unused), six),
        Err(ReadError::TooLarge { what: "triangles", limit: 6 })
    );
    // Four placements of an empty mesh, against a triangle limit of four.
    let empty = "<object id=\"1\"><mesh><vertices/><triangles/></mesh></object>\
                 <object id=\"2\"><components><component objectid=\"1\"/>\
                 <component objectid=\"1\"/><component objectid=\"1\"/></components></object>";
    let placed = package(model(
        "",
        &format!("<resources>{empty}</resources><build><item objectid=\"2\"/></build>"),
    ));
    let four = ROOMY.with_triangles(4);
    assert_eq!(read(&placed, four).map(|m| m.triangles.len()), Ok(0));
    let three = ROOMY.with_triangles(3);
    assert!(matches!(read(&placed, three), Err(ReadError::TooLarge { what: "triangles", .. })));
}
