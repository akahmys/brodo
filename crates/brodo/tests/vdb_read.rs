//! Format tier (TESTING.md §1) for reading OpenVDB files (docs/formats.md §9.7, ADR-0047):
//! files OpenVDB 12.0.0 and 14.0.0 wrote are read as OpenVDB's own reading gives them, as
//! `spikes/vdbread/oracle/`'s `oracle expect` listed it into `vdb_read_expect.txt`; a grid
//! brodo writes reads back the same; what §9.7 refuses is refused.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use brodo::error::VdbError;
use brodo::field::Field;
use brodo::meta::{GridMeta, SDF_KEY, SDF_VERSION};
use brodo::topology::Topology;
use brodo::tree::TileLevel;
use brodo::vdb::{VdbEntry, VdbField, VdbFile, VdbValue, from_bytes, read, to_bytes};

const BY12: &[u8] = include_bytes!("data/vdb_by12_v224.bin");
const BY14: &[u8] = include_bytes!("data/vdb_by14_v225.bin");
const NONE: &[u8] = include_bytes!("data/vdb_by14_none.bin");
const EXPECT: &str = include_str!("data/vdb_read_expect.txt");
const NONE_EXPECT: &str = include_str!("data/vdb_read_none_expect.txt");

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_of<T: VdbValue>(values: &[T]) -> String {
    let mut out = Vec::new();
    for v in values {
        v.put(&mut out);
    }
    hex(&out)
}

fn floats(xs: impl IntoIterator<Item = f64>) -> String {
    hex(&xs.into_iter().flat_map(f64::to_le_bytes).collect::<Vec<u8>>())
}

fn level(l: TileLevel) -> &'static str {
    match l {
        TileLevel::Root => "root",
        TileLevel::Upper => "upper",
        TileLevel::Lower => "lower",
    }
}

/// The lines `oracle expect` prints for a grid, from what brodo read.
fn lines_of<T: VdbValue>(meta: &GridMeta, t: &Topology, f: &Field<T>) -> Vec<String> {
    let mut out = vec![format!("grid {} {}", meta.name, T::TREE_TYPE)];
    for (k, v) in &meta.extra {
        out.push(format!("meta {k} {}", hex(v.as_bytes())));
    }
    out.push(format!("voxel_size {}", floats(meta.voxel_size)));
    out.push(format!("transform {}", floats(meta.transform.iter().flatten().copied())));
    out.push(format!("background {}", hex_of(&[f.background()])));
    let mut rows = Vec::new();
    for n in t.leaf_numbers() {
        let [x, y, z] = t.leaf_origin(n).unwrap();
        rows.push(format!("leaf {x} {y} {z} {}", hex_of(f.leaf(t, n).unwrap())));
    }
    for n in t.tile_numbers() {
        let (l, [x, y, z]) = t.tile(n).unwrap();
        rows.push(format!("tile {} {x} {y} {z} {}", level(l), hex_of(&[f.tile(t, n).unwrap()])));
    }
    rows.sort();
    out.extend(rows);
    out
}

/// Each grid's lines by name, and the names skipped.
type Listing = (BTreeMap<String, Vec<String>>, BTreeSet<String>);

fn listing_of(file: &VdbFile) -> Listing {
    let mut grids = BTreeMap::new();
    for g in &file.grids {
        let lines = match &g.field {
            VdbField::F32(f) => lines_of(&g.meta, &g.topology, f),
            VdbField::F64(f) => lines_of(&g.meta, &g.topology, f),
            _ => panic!("a value type this test does not know"),
        };
        grids.insert(g.meta.name.clone(), lines);
    }
    (grids, file.skipped.iter().cloned().collect())
}

fn listing_expected(text: &str) -> Listing {
    let (mut grids, mut skipped) = (BTreeMap::new(), BTreeSet::new());
    let mut name = String::new();
    for line in text.lines() {
        if let Some(n) = line.strip_prefix("skipped ") {
            skipped.insert(n.to_owned());
        } else {
            if let Some(rest) = line.strip_prefix("grid ") {
                name = rest.split(' ').next().unwrap().to_owned();
            }
            grids.entry(name.clone()).or_insert_with(Vec::new).push(line.to_owned());
        }
    }
    (grids, skipped)
}

fn same_listing(bytes: &[u8], expect: &str) {
    let (got, want) = (listing_of(&from_bytes(bytes, "m").unwrap()), listing_expected(expect));
    assert_eq!(got.1, want.1, "skipped");
    assert_eq!(got.0.keys().collect::<Vec<_>>(), want.0.keys().collect::<Vec<_>>(), "grids");
    for (name, lines) in &want.0 {
        assert_eq!(&got.0[name], lines, "grid {name}");
    }
}

#[test]
fn files_openvdb_12_and_14_wrote_read_as_openvdb_reads_them() {
    same_listing(BY12, EXPECT);
    same_listing(BY14, EXPECT);
}

#[test]
fn an_uncompressed_file_reads_as_openvdb_reads_it() {
    same_listing(NONE, NONE_EXPECT);
}

/// A grid brodo writes, its values distinct and its transform exact in binary, so that the
/// half voxel added and taken away leaves it as it was.
fn written<T: VdbValue>(background: T, of: impl Fn(u32) -> T) -> (GridMeta, Topology, Field<T>) {
    let empty = Topology::new();
    let mut e = empty.edit();
    for v in [[-1, 0, 0], [0, 0, 0], [9, 0, 0], [3, 130, 5], [4096, 7, 9]] {
        e.activate(v).unwrap();
    }
    for (l, v) in [
        (TileLevel::Lower, [8, 8, 8]),
        (TileLevel::Upper, [128, 0, 0]),
        (TileLevel::Root, [0, 8192, 0]),
    ] {
        e.add_tile(l, v).unwrap();
    }
    let t = e.commit().unwrap().0;
    let mut f = Field::new(&t, background);
    let mut k = 0;
    for n in t.leaf_numbers() {
        for v in f.leaf_mut(&t, n).unwrap() {
            *v = of(k);
            k += 1;
        }
    }
    for n in t.tile_numbers() {
        f.set_tile(&t, n, of(k)).unwrap();
        k += 1;
    }
    let mut m = GridMeta::new("g", [0.5, 0.5, 0.75], "mm").unwrap();
    m.transform = [[0.5, 0.0, 0.0, -12.5], [0.0, 0.5, 0.0, 3.0], [0.0, 0.0, 0.75, 0.5]];
    m.extra.insert(SDF_KEY.to_owned(), SDF_VERSION.to_owned());
    (m, t, f)
}

fn round_trip<T: VdbValue + PartialEq + std::fmt::Debug>(
    m: &GridMeta,
    t: &Topology,
    f: &Field<T>,
) -> VdbFile {
    let bytes = to_bytes(&[VdbEntry::new(m, t, f).unwrap()]).unwrap();
    from_bytes(&bytes, "mm").unwrap()
}

#[test]
fn a_grid_brodo_writes_reads_back_the_same() {
    let (m, t, f) = written(3.0_f32, |k| k as f32 * 0.5 - 900.0);
    let file = round_trip(&m, &t, &f);
    let (m2, t2, f2) = (&file.grids[0].meta, &file.grids[0].topology, &file.grids[0].field);
    let VdbField::F32(f2) = f2 else { panic!("not a float grid") };
    let mut want = m.clone();
    want.extra = BTreeMap::from([("class".to_owned(), "level set".to_owned())]);
    assert_eq!(m2, &want);
    assert_eq!(lines_of(m2, t2, f2), lines_of(&want, &t, &f));
    let (m, t, g) = written(-1.0_f64, |k| f64::from(k) * 0.25 + 1e-12 * f64::from(k % 7));
    let file = round_trip(&m, &t, &g);
    let VdbField::F64(g2) = &file.grids[0].field else { panic!("not a double grid") };
    assert_eq!(
        lines_of(&file.grids[0].meta, &file.grids[0].topology, g2),
        lines_of(&file.grids[0].meta, &t, &g)
    );
}

/// Where the first grid's body starts, and its compression flags with it.
fn first_body(bytes: &[u8]) -> usize {
    let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    let mut at = 8 + 4 + 8 + 1 + 36;
    assert_eq!(u32_at(at), 0, "this test reads a file without file metadata");
    at += 8;
    for _ in 0..3 {
        at += 4 + u32_at(at);
    }
    at + 24
}

#[test]
fn zip_another_version_and_a_longer_file_are_refused() {
    let mut zip = BY14.to_vec();
    let at = first_body(&zip);
    zip[at] |= 1;
    assert!(matches!(from_bytes(&zip, "m"), Err(VdbError::Unsupported("zip compression"))));
    let mut old = BY14.to_vec();
    old[8] = 223;
    assert!(matches!(from_bytes(&old, "m"), Err(VdbError::Version(223))));
    let mut longer = BY14.to_vec();
    longer.push(0);
    assert!(matches!(from_bytes(&longer, "m"), Err(VdbError::Malformed("file's end"))));
}

#[test]
fn every_truncation_is_refused() {
    for len in (0..BY14.len()).step_by(997) {
        assert!(
            matches!(from_bytes(&BY14[..len], "m"), Err(VdbError::Malformed(_))),
            "{len} bytes read"
        );
    }
}

#[test]
fn a_file_above_the_limit_is_refused_before_it_is_read() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/vdb_by14_v225.bin");
    let limit = BY14.len() as u64 - 1;
    assert!(matches!(read(&path, "m", limit), Err(VdbError::FileTooLarge { .. })));
    assert_eq!(
        listing_of(&read(&path, "m", limit + 1).unwrap()),
        listing_of(&from_bytes(BY14, "m").unwrap())
    );
}
