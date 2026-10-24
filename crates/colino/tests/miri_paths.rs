//! A path through code that rests on unsafe code in colino's dependencies, small enough for
//! Miri (TESTING.md T7, ADR-0008): reading a 3MF package, whose XML `quick-xml` scans with
//! `memchr`. `rawzip`, `miniz_oxide` and `quick-xml` forbid unsafe code of their own. The
//! package is written by `rawzip`'s own writer, after its documentation's example. It runs
//! in the gate as any test; before a milestone closes, Miri runs it (TESTING.md §3).

use std::io::Write;

use colino::mesh::Limits;
use colino::threemf::read;

const CORE: &str = "http://schemas.microsoft.com/3dmanufacturing/core/2015/02";
const RELS: &str = "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
    <Relationship Target=\"/3D/3dmodel.model\" Id=\"rel0\" \
    Type=\"http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel\"/></Relationships>";
const TYPES: &str =
    "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"/>";

/// A package of the relationships, the content types and a model of one tetrahedron.
fn package() -> Vec<u8> {
    let model = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<model xmlns=\"{CORE}\"><resources>\
         <object id=\"1\" type=\"model\"><mesh><vertices>\
         <vertex x=\"0\" y=\"0\" z=\"0\"/><vertex x=\"1.5\" y=\"0\" z=\"0\"/>\
         <vertex x=\"0\" y=\"2.5\" z=\"0\"/><vertex x=\"0\" y=\"0\" z=\"-3.25\"/></vertices>\
         <triangles><triangle v1=\"0\" v2=\"2\" v3=\"1\"/><triangle v1=\"0\" v2=\"1\" v3=\"3\"/>\
         <triangle v1=\"0\" v2=\"3\" v3=\"2\"/><triangle v1=\"1\" v2=\"2\" v3=\"3\"/></triangles>\
         </mesh></object></resources><build><item objectid=\"1\"/></build></model>"
    );
    let mut output = std::io::Cursor::new(Vec::new());
    let mut archive = rawzip::ZipArchiveWriter::new(&mut output);
    for (name, data) in [
        ("[Content_Types].xml", TYPES.as_bytes()),
        ("_rels/.rels", RELS.as_bytes()),
        ("3D/3dmodel.model", model.as_bytes()),
    ] {
        let (mut entry, config) = archive.new_file(name).start().unwrap();
        let mut writer = config.wrap(&mut entry);
        writer.write_all(data).unwrap();
        let (_, descriptor) = writer.finish().unwrap();
        entry.finish(descriptor).unwrap();
    }
    archive.finish().unwrap();
    output.into_inner()
}

#[test]
fn miri_a_3mf_package_reads() {
    let limits = Limits::new(16, 16, 1 << 16);
    let mesh = read(&package(), limits).unwrap();
    let bits: Vec<u64> = mesh.vertices.iter().flatten().map(|c| c.to_bits()).collect();
    let want: Vec<u64> = [0.0, 0.0, 0.0, 1.5, 0.0, 0.0, 0.0, 2.5, 0.0, 0.0, 0.0, -3.25]
        .iter()
        .map(|c: &f64| c.to_bits())
        .collect();
    assert_eq!(bits, want);
    assert_eq!(mesh.triangles, [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]]);
}
