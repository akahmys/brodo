//! Format tier (TESTING.md §1) for OpenVDB files (docs/formats.md §9, ADR-0046): the golden
//! file keeps its bytes, and `scripts/audit/formats.py`, a reader written from the document
//! alone, reads from it every grid's metadata, transform, leaves and tiles as brodo holds
//! them. OpenVDB's own reading of the files is checked by hand (`spikes/vdb/`), not here.

use std::path::Path;
use std::process::Command;

use brodo::error::VdbError;
use brodo::field::{Field, Value};
use brodo::meta::{Bounds, GridMeta, SDF_KEY, SDF_VERSION};
use brodo::topology::Topology;
use brodo::tree::{Coord, TileLevel};
use brodo::vdb::{VdbEntry, VdbValue, to_bytes, write};

const GOLDEN: &[u8] = include_bytes!("data/vdb_v225.bin");

fn build(leaves: &[Coord], tiles: &[(TileLevel, Coord)]) -> Topology {
    let empty = Topology::new();
    let mut e = empty.edit();
    for v in leaves {
        e.activate(*v).unwrap();
    }
    for (l, v) in tiles {
        e.add_tile(*l, *v).unwrap();
    }
    e.commit().unwrap().0
}

/// Leaves in two upper nodes, two in one lower node, and a tile at each level.
fn topology() -> Topology {
    build(
        &[[-1, 0, 0], [0, 0, 0], [9, 0, 0], [3, 130, 5], [4096, 7, 9]],
        &[
            (TileLevel::Lower, [8, 8, 8]),
            (TileLevel::Upper, [128, 0, 0]),
            (TileLevel::Root, [0, 8192, 0]),
        ],
    )
}

/// A field whose every value differs from the others, so that a value written in another
/// voxel's place shows.
fn numbered<T: Value>(t: &Topology, background: T, of: impl Fn(u32) -> T) -> Field<T> {
    let mut f = Field::new(t, background);
    let mut k = 0;
    for n in t.leaf_numbers() {
        for v in f.leaf_mut(t, n).unwrap() {
            *v = of(k);
            k += 1;
        }
    }
    for n in t.tile_numbers() {
        f.set_tile(t, n, of(k)).unwrap();
        k += 1;
    }
    f
}

/// Metadata whose transform has every entry distinct, so that a transposed matrix shows.
fn meta(name: &str, sdf: bool) -> GridMeta {
    let mut m = GridMeta::new(name, [0.5; 3], "mm").unwrap();
    m.transform = [[0.5, 0.125, 0.0, -12.5], [0.0, 0.5, 0.25, 3.0], [0.0625, 0.0, 0.75, 0.5]];
    if sdf {
        m.extra.insert(SDF_KEY.to_owned(), SDF_VERSION.to_owned());
    }
    m.extra.insert("source".to_owned(), "test".to_owned());
    m
}

fn fields(t: &Topology) -> (Field<f32>, Field<f64>) {
    let f = numbered(t, 3.0_f32, |k| k as f32 * 0.5 - 900.0);
    let g = numbered(t, -1.0_f64, |k| f64::from(k) * 0.25 + 1e-12 * f64::from(k % 7));
    (f, g)
}

fn golden() -> Vec<u8> {
    let t = topology();
    let (f, g) = fields(&t);
    let empty = Topology::new();
    let entries = [
        VdbEntry::new(&meta("sdf", true), &t, &f).unwrap(),
        VdbEntry::new(&meta("density", false), &t, &g).unwrap(),
        VdbEntry::new(&meta("empty", false), &empty, &Field::new(&empty, 0.0_f32)).unwrap(),
    ];
    to_bytes(&entries).unwrap()
}

#[test]
fn file_version_225_keeps_its_bytes() {
    let bytes = golden();
    if bytes != GOLDEN {
        let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("vdb_v225.bin");
        std::fs::write(&path, &bytes).unwrap();
        panic!("the bytes brodo writes as .vdb changed; they are at {}", path.display());
    }
}

#[test]
fn the_same_grids_give_the_same_bytes_on_any_thread_count() {
    let on = |threads| {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
        pool.install(golden)
    };
    assert_eq!(on(1), on(4));
}

#[test]
fn a_written_file_holds_the_bytes() {
    let t = topology();
    let (f, _) = fields(&t);
    let entries = [VdbEntry::new(&meta("sdf", true), &t, &f).unwrap()];
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("written.vdb");
    write(&path, &entries).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), to_bytes(&entries).unwrap());
}

#[test]
fn two_grids_of_one_name_are_refused() {
    let t = topology();
    let (f, g) = fields(&t);
    let entries = [
        VdbEntry::new(&meta("a", false), &t, &f).unwrap(),
        VdbEntry::new(&meta("a", true), &t, &g).unwrap(),
    ];
    assert!(matches!(to_bytes(&entries), Err(VdbError::DuplicateName(n)) if n == "a"));
}

#[test]
fn a_field_of_another_topology_and_bad_metadata_are_refused() {
    let t = topology();
    let other = build(&[[0, 0, 0]], &[]);
    let f = Field::new(&other, 0.0_f32);
    assert!(matches!(VdbEntry::new(&meta("g", false), &t, &f), Err(VdbError::Field(_))));
    let mut m = meta("g", false);
    m.transform[2][1] = f64::INFINITY;
    let f = Field::new(&t, 0.0_f32);
    assert!(matches!(VdbEntry::new(&m, &t, &f), Err(VdbError::Meta(_))));
}

/// The document's reader on the golden file, its lines between `file` and `end`.
fn read_golden() -> Vec<String> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/audit/formats.py");
    let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/vdb_v225.bin");
    let out = Command::new("python3").arg("-I").arg(script).arg(golden).output().unwrap();
    assert!(out.status.success(), "the document's reader failed");
    let text = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<String> = text.lines().skip(1).map(str::to_owned).collect();
    assert_eq!(lines.last().map(String::as_str), Some("end"), "{text}");
    assert!(!lines.iter().any(|l| l.starts_with("refused")), "{text}");
    lines
}

/// The reader's lines for each grid, after its `vdb` line.
fn grids(lines: &[String]) -> Vec<(Vec<String>, Vec<String>)> {
    let mut out: Vec<(Vec<String>, Vec<String>)> = Vec::new();
    for line in lines.iter().filter(|l| *l != "end") {
        let w: Vec<String> = line.split(' ').map(str::to_owned).collect();
        if w[0] == "vdb" {
            out.push((w, Vec::new()));
        } else {
            out.last_mut().unwrap().1.push(line.clone());
        }
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    let h: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    if h.is_empty() { "-".to_owned() } else { h }
}

fn bytes_of<T: VdbValue>(values: &[T]) -> String {
    let mut out = Vec::new();
    for v in values {
        v.put(&mut out);
    }
    hex(&out)
}

/// docs/formats.md §9.3's matrix, from brodo's map.
fn matrix(a: &[[f64; 4]; 3]) -> Vec<String> {
    let mut m = [[0.0_f64; 4]; 4];
    for r in 0..3 {
        for c in 0..3 {
            m[c][r] = a[r][c];
        }
        m[3][r] = a[r][3] + 0.5 * (a[r][0] + a[r][1] + a[r][2]);
    }
    m[3][3] = 1.0;
    m.iter().flatten().map(|x| hex(&x.to_le_bytes())).collect()
}

fn level(name: &str) -> TileLevel {
    match name {
        "root" => TileLevel::Root,
        "upper" => TileLevel::Upper,
        _ => TileLevel::Lower,
    }
}

fn coord(words: &[String]) -> Coord {
    [0, 1, 2].map(|k| words[k].parse().unwrap())
}

/// Checks one grid as the reader printed it against the topology, field and metadata
/// written.
fn same_grid<T: VdbValue>(lines: &[String], m: &GridMeta, t: &Topology, f: &Field<T>, class: &str) {
    let mut meta = std::collections::BTreeMap::new();
    let (mut leaves, mut tiles) = (0, 0);
    for line in lines {
        let w: Vec<String> = line.split(' ').map(str::to_owned).collect();
        match w[0].as_str() {
            "meta" => {
                meta.insert(w[1].clone(), (w[2].clone(), w[3].clone()));
            }
            "transform" => assert_eq!(w[1..], matrix(&m.transform), "transform"),
            "background" => assert_eq!(w[1], bytes_of(&[f.background()]), "background"),
            "leaf" => {
                let at = coord(&w[1..4]);
                let n = t.leaf_numbers().find(|n| t.leaf_origin(*n).unwrap() == at).unwrap();
                assert_eq!(w[4], bytes_of(f.leaf(t, n).unwrap()), "leaf at {at:?}");
                leaves += 1;
            }
            "tile" => {
                let at = coord(&w[2..5]);
                let n =
                    t.tile_numbers().find(|n| t.tile(*n).unwrap() == (level(&w[1]), at)).unwrap();
                assert_eq!(w[5], bytes_of(&[f.tile(t, n).unwrap()]), "tile at {at:?}");
                tiles += 1;
            }
            other => panic!("unread line {other}"),
        }
    }
    assert_eq!((leaves, tiles), (t.leaf_count(), t.tile_count()));
    let (min, max) = match Bounds::of(t).unwrap() {
        Bounds::Box { min, max } => (min, max),
        Bounds::Empty => ([i32::MAX; 3], [i32::MIN; 3]),
    };
    let vec3i = |c: Coord| hex(&c.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>());
    let edges = t.tile_numbers().map(|n| i64::from(t.tile(n).unwrap().0.edge()).pow(3));
    let voxels = 512 * i64::try_from(t.leaf_count()).unwrap() + edges.sum::<i64>();
    let want = [
        ("class", "string", hex(class.as_bytes())),
        ("file_bbox_max", "vec3i", vec3i(max)),
        ("file_bbox_min", "vec3i", vec3i(min)),
        ("file_compression", "string", hex(b"active values")),
        ("file_mem_bytes", "int64", hex(&0_i64.to_le_bytes())),
        ("file_voxel_count", "int64", hex(&voxels.to_le_bytes())),
        ("name", "string", hex(m.name.as_bytes())),
    ];
    let want: std::collections::BTreeMap<String, (String, String)> =
        want.into_iter().map(|(k, t, v)| (k.to_owned(), (t.to_owned(), v))).collect();
    assert_eq!(meta, want, "metadata");
}

#[test]
fn the_document_s_reader_reads_the_golden_file_as_brodo_wrote_it() {
    let t = topology();
    let (f, g) = fields(&t);
    let empty = Topology::new();
    let e = Field::new(&empty, 0.0_f32);
    let got = grids(&read_golden());
    let heads: Vec<String> = got.iter().map(|(w, _)| w[1..].join(" ")).collect();
    assert_eq!(
        heads,
        [
            format!("{} Tree_float_5_4_3", hex(b"sdf")),
            format!("{} Tree_double_5_4_3", hex(b"density")),
            format!("{} Tree_float_5_4_3", hex(b"empty")),
        ]
    );
    same_grid(&got[0].1, &meta("sdf", true), &t, &f, "level set");
    same_grid(&got[1].1, &meta("density", false), &t, &g, "unknown");
    same_grid(&got[2].1, &meta("empty", false), &empty, &e, "unknown");
}
