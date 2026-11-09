//! Format tier (TESTING.md §1) for the format document (ADR-0037): `scripts/audit/formats.py`,
//! a reader written from `docs/formats.md` alone, reads every golden file as the crates
//! read it, value for value, and accepts or refuses each damaged copy of them as the
//! crates do, an archive's region (§8.6) among them. A difference means the document leaves out or misstates what a reader needs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(feature = "archive")]
use brodo::archive::region_from_bytes;
use brodo::cache::{CacheEntry, CacheFile};
use brodo::field::Field;
use brodo::field::Stored;
use brodo::hermite::HermiteView;
use brodo::meta::Bounds;
use brodo::tree::TileLevel;
use brodo::view::{GridView, ViewLocation};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

/// The golden files, the archive's where the crate reads it.
const GOLDEN: &[&str] = &[
    "frozen_v2_f32.bin",
    "frozen_v2_f64x3.bin",
    "cache_v2.bin",
    "hermite_v2.bin",
    "hermite_v3.bin",
    #[cfg(feature = "archive")]
    "archive_v2.bin",
    #[cfg(feature = "archive")]
    "archive_v2_grids.bin",
    #[cfg(feature = "archive")]
    "archive_v2_quantised.bin",
];

/// A cache file as the crates read the bytes: a cache file's, or an archive's decompressed.
fn cache_file(bytes: &[u8]) -> Result<CacheFile, brodo::error::CacheError> {
    #[cfg(feature = "archive")]
    if bytes.starts_with(b"BRDA") {
        return brodo::archive::from_bytes(bytes, u64::MAX);
    }
    brodo::cache::from_bytes(bytes)
}

/// A 32-byte aligned copy of some bytes, as a file read into memory would be.
#[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Clone, Copy)]
#[repr(C, align(32))]
struct Block([u8; 32]);

fn aligned(bytes: &[u8]) -> Vec<Block> {
    let mut out = vec![Block([0; 32]); bytes.len().div_ceil(32)];
    out.as_mut_bytes()[..bytes.len()].copy_from_slice(bytes);
    out
}

fn data(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name)
}

/// The document's reader, run with `args`, `input` on its standard input.
fn reader(args: &[&str], input: &str) -> String {
    use std::io::Write;
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/audit/formats.py");
    let mut child = Command::new("python3")
        .arg("-I")
        .arg(script)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("python3 runs the document's reader");
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "the document's reader failed");
    String::from_utf8(out.stdout).unwrap()
}

/// The reader's report on each file: its lines between `file` and `end`, or `refused`.
fn reports(out: &str) -> Vec<(String, Vec<String>)> {
    let mut all: Vec<(String, Vec<String>)> = Vec::new();
    for line in out.lines() {
        if let Some(name) = line.strip_prefix("file ") {
            all.push((name.to_owned(), Vec::new()));
        } else if line != "end" {
            all.last_mut().unwrap().1.push(line.to_owned());
        }
    }
    all
}

fn hex(bytes: &[u8]) -> String {
    let h: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    if h.is_empty() { "-".to_owned() } else { h }
}

fn level(name: &str) -> TileLevel {
    match name {
        "root" => TileLevel::Root,
        "upper" => TileLevel::Upper,
        _ => TileLevel::Lower,
    }
}

fn coord(words: &[&str]) -> [i32; 3] {
    [0, 1, 2].map(|k| words[k].parse().unwrap())
}

/// Checks one grid's background, leaves and tiles, as the reader printed them, against a
/// view of it.
fn same_values<T: Stored>(view: &GridView<'_, T>, lines: &[String]) {
    let (mut leaves, mut tiles) = (0_u32, 0_usize);
    for line in lines {
        let w: Vec<&str> = line.split(' ').collect();
        match w[0] {
            "background" => assert_eq!(w[1], hex(view.background().as_bytes()), "background"),
            "leaf" => {
                let at = coord(&w[1..4]);
                assert_eq!(view.locate(at).unwrap(), ViewLocation::Leaf(leaves), "leaf at {at:?}");
                assert_eq!(w[4], hex(view.leaf(leaves).unwrap().as_bytes()), "leaf {leaves}");
                leaves += 1;
            }
            "voxel" => {
                let at = coord(&w[1..4]);
                assert_eq!(w[4], hex(view.get(at).unwrap().as_bytes()), "voxel {at:?}");
            }
            "tile" => {
                let at = coord(&w[2..5]);
                let ViewLocation::Tile(_, l) = view.locate(at).unwrap() else {
                    panic!("no tile at {at:?}")
                };
                assert_eq!(l, level(w[1]), "tile level at {at:?}");
                assert_eq!(w[5], hex(view.get(at).unwrap().as_bytes()), "tile at {at:?}");
                tiles += 1;
            }
            _ => {}
        }
    }
    assert_eq!(leaves as usize, view.leaf_count(), "leaf count");
    assert_eq!(tiles, view.tile_count(), "tile count");
}

fn same_grid(bytes: &[u8], code: u32, lines: &[String]) {
    match code {
        1 => same_values(&GridView::<f32>::new(bytes).unwrap(), lines),
        2 => same_values(&GridView::<f64>::new(bytes).unwrap(), lines),
        3 => same_values(&GridView::<u8>::new(bytes).unwrap(), lines),
        4 => same_values(&GridView::<u32>::new(bytes).unwrap(), lines),
        5 => same_values(&GridView::<[f32; 3]>::new(bytes).unwrap(), lines),
        6 => same_values(&GridView::<[f64; 3]>::new(bytes).unwrap(), lines),
        _ => panic!("value type {code}"),
    }
}

fn same_cache_grid(file: &CacheFile, i: usize, lines: &[String]) {
    macro_rules! values {
        ($t:ty) => {
            same_values(&file.grids()[i].view::<$t>().unwrap(), lines)
        };
    }
    let m = file.grids()[i].meta();
    let mut pairs = Vec::new();
    for line in lines {
        let w: Vec<&str> = line.split(' ').collect();
        match w[0] {
            "name" => assert_eq!(w[1], hex(m.name.as_bytes()), "name"),
            "unit" => assert_eq!(w[1], hex(m.unit.as_bytes()), "unit"),
            "voxel_size" => {
                let ours: Vec<String> =
                    m.voxel_size.iter().map(|x| hex(&x.to_le_bytes())).collect();
                assert_eq!(w[1..], ours, "voxel size");
            }
            "transform" => {
                let flat = m.transform.as_flattened();
                let ours: Vec<String> = flat.iter().map(|x| hex(&x.to_le_bytes())).collect();
                assert_eq!(w[1..], ours, "transform");
            }
            "bounds" => {
                let ours = match file.grids()[i].bounds() {
                    Bounds::Empty => vec!["empty".to_owned()],
                    Bounds::Box { min, max } => {
                        min.iter().chain(&max).map(ToString::to_string).collect()
                    }
                };
                assert_eq!(w[1..], ours, "bounds");
            }
            "pair" => pairs.push((w[1].to_owned(), w[2].to_owned())),
            _ => {}
        }
    }
    let ours: Vec<_> =
        m.extra.iter().map(|(k, v)| (hex(k.as_bytes()), hex(v.as_bytes()))).collect();
    assert_eq!(pairs, ours, "free pairs");
    match file.grids()[i].value_type().code() {
        1 => values!(f32),
        2 => values!(f64),
        3 => values!(u8),
        4 => values!(u32),
        5 => values!([f32; 3]),
        6 => values!([f64; 3]),
        code => panic!("value type {code}"),
    }
}

/// The grids' lines in a report, each grid's after its `grid` line.
fn grids(lines: &[String]) -> Vec<(u32, Vec<String>)> {
    let mut out: Vec<(u32, Vec<String>)> = Vec::new();
    for line in lines.iter().filter(|l| !l.starts_with("hermite ")) {
        if let Some(rest) = line.strip_prefix("grid ") {
            out.push((rest.split(' ').nth(1).unwrap().parse().unwrap(), Vec::new()));
        } else {
            out.last_mut().unwrap().1.push(line.clone());
        }
    }
    out
}

/// What the crates make of a file: accepted or not, and each Hermite set's verdict, as the
/// reader prints them.
fn crates_read(bytes: &[u8]) -> Vec<String> {
    if bytes.starts_with(b"BRDG") {
        let copy = aligned(bytes);
        let bytes = &copy.as_bytes()[..bytes.len()];
        let accepted = GridView::<f32>::new(bytes).is_ok()
            || GridView::<f64>::new(bytes).is_ok()
            || GridView::<u8>::new(bytes).is_ok()
            || GridView::<u32>::new(bytes).is_ok()
            || GridView::<[f32; 3]>::new(bytes).is_ok()
            || GridView::<[f64; 3]>::new(bytes).is_ok();
        return vec![if accepted { "accepted" } else { "refused" }.to_owned()];
    }
    let Ok(file) = cache_file(bytes) else { return vec!["refused".to_owned()] };
    let mut out = vec!["accepted".to_owned()];
    let grids = file.grids();
    let extras = grids.iter().map(|g| &g.meta().extra);
    let bytes_entries = file.bytes_entries();
    let mut sets: Vec<String> = extras
        .chain(bytes_entries.iter().map(|b| b.extra))
        .filter_map(|extra| {
            extra.get("brodo.hermite.set").or_else(|| extra.get("hermite.set")).cloned()
        })
        .collect();
    sets.sort();
    sets.dedup();
    for set in sets {
        out.push(match HermiteView::find(&file, &set) {
            Ok(v) => {
                let names: Vec<String> =
                    v.group_names().iter().map(|n| hex(n.as_bytes())).collect();
                format!("hermite {} ok {}", hex(set.as_bytes()), names.join(" "))
                    .trim_end()
                    .to_owned()
            }
            Err(_) => format!("hermite {} refused", hex(set.as_bytes())),
        });
    }
    out
}

/// The reader's brief lines, its reasons for refusing dropped.
fn verdict(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .map(|l| match l.find(" refused") {
            Some(at) if l.starts_with("hermite ") => l[..at + " refused".len()].to_owned(),
            _ if l.starts_with("refused") => "refused".to_owned(),
            _ => l.trim_end().to_owned(),
        })
        .collect()
}

#[test]
fn the_document_s_reader_reads_every_golden_file_as_the_crates_do() {
    let paths: Vec<String> = GOLDEN.iter().map(|g| data(g).display().to_string()).collect();
    let args: Vec<&str> = paths.iter().map(String::as_str).collect();
    let out = reports(&reader(&args, ""));
    assert_eq!(out.len(), GOLDEN.len());
    for ((name, lines), path) in out.iter().zip(&paths) {
        let bytes = std::fs::read(path).unwrap();
        assert!(!lines.iter().any(|l| l.starts_with("refused")), "{name}: {lines:?}");
        let grids = grids(lines);
        if bytes.starts_with(b"BRDG") {
            assert_eq!(grids.len(), 1);
            same_grid(aligned(&bytes).as_bytes(), grids[0].0, &grids[0].1);
            continue;
        }
        let file = cache_file(&bytes).unwrap();
        assert_eq!(grids.len(), file.grids().len(), "{name}: grid count");
        for (i, (code, lines)) in grids.iter().enumerate() {
            assert_eq!(*code, file.grids()[i].value_type().code(), "{name}: grid {i}'s type");
            same_cache_grid(&file, i, lines);
        }
        let hermite: Vec<String> =
            lines.iter().filter(|l| l.starts_with("hermite ")).cloned().collect();
        assert_eq!(verdict(&hermite), crates_read(&bytes)[1..], "{name}: Hermite sets");
    }
}

/// The golden Hermite set's entries, each as read, but with `edit` given its keys: its
/// sign grid is entry 0 and its crossings entry 1.
fn golden_set_as(edit: impl Fn(usize, &mut BTreeMap<String, String>)) -> Vec<CacheEntry> {
    let file = brodo::cache::from_bytes(&std::fs::read(data("hermite_v3.bin")).unwrap()).unwrap();
    let g = file.grids()[0];
    let mut meta = g.meta().clone();
    edit(0, &mut meta.extra);
    let (t, signs) = g.thaw(u64::MAX).unwrap();
    let signs = Field::<u8>::try_from(signs).unwrap();
    let b = file.bytes_entries()[0];
    let mut extra = b.extra.clone();
    edit(1, &mut extra);
    vec![
        CacheEntry::with_reserved_keys(&meta, &t, &signs).unwrap(),
        CacheEntry::bytes_with_reserved_keys(b.name, extra, b.bytes.to_vec()),
    ]
}

/// The document's reader and the crates name the same Hermite sets of a file, those its
/// entries of bytes name among them, and find each alike; an earlier version's key on an
/// entry of this version's names no earlier set (docs/formats.md §4.2, rule 1; issue 15).
#[test]
fn the_document_s_reader_finds_the_sets_the_crates_find() {
    let named = |set: &'static str| {
        move |_: usize, m: &mut BTreeMap<String, String>| {
            m.insert("brodo.hermite.set".into(), set.into());
        }
    };
    // Set "golden" carries a free `hermite.set` naming set "b", which is of this version.
    let mut beside = golden_set_as(|_, m| {
        m.insert("hermite.set".into(), "b".into());
        m.insert("hermite.version".into(), "2".into());
    });
    beside.extend(golden_set_as(named("b")));
    // Set "c" is named by its crossings alone, its sign grid gone.
    let alone = vec![golden_set_as(named("c")).remove(1)];
    let expected = [
        vec!["hermite 62 ok - 61 62", "hermite 676f6c64656e ok - 61 62"],
        vec!["hermite 63 refused"],
    ];
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"));
    for ((entries, name), expected) in
        [(beside, "beside.bin"), (alone, "alone.bin")].into_iter().zip(expected)
    {
        let bytes = brodo::cache::to_bytes(&entries).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, &bytes).unwrap();
        let ours = crates_read(&bytes);
        assert_eq!(ours[1..], expected, "{name}: the crates");
        let out = reports(&reader(&[&path.display().to_string()], ""));
        let theirs: Vec<String> =
            out[0].1.iter().filter(|l| l.starts_with("hermite ")).cloned().collect();
        assert_eq!(verdict(&theirs), ours[1..], "{name}: the document's reader");
    }
}

/// The offsets of the words damaged in a golden file: the first three blocks of the file
/// and of each of its first two grids' metadata and buffers, and every 127th word.
fn damaged_words(bytes: &[u8]) -> Vec<usize> {
    let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    let mut starts = vec![0];
    // A cache file's metadata and buffers; an archive's metadata and chunk tables.
    if bytes.starts_with(b"BRDC") || bytes.starts_with(b"BRDA") {
        for i in 0..word(8).min(2) {
            starts.push(word(32 * (1 + i)) * 32);
            starts.push(word(32 * (1 + i) + 8) * 32);
        }
    }
    let mut at: Vec<usize> = starts.iter().flat_map(|s| (*s..*s + 96).step_by(4)).collect();
    at.extend((0..bytes.len()).step_by(4 * 127));
    at.sort_unstable();
    at.dedup();
    at.retain(|a| a + 4 <= bytes.len());
    at
}

#[test]
fn the_document_s_reader_accepts_and_refuses_damaged_files_as_the_crates_do() {
    let mut cases = 0;
    for golden in GOLDEN.iter().copied() {
        let path = data(golden);
        let bytes = std::fs::read(&path).unwrap();
        let mut patches = String::new();
        let mut ours = Vec::new();
        for at in damaged_words(&bytes) {
            let was = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
            for value in [0, 0xFFFF_FFFF, was ^ 1, was ^ 0x8000_0000] {
                if value == was {
                    continue;
                }
                let mut damaged = bytes.clone();
                damaged[at..at + 4].copy_from_slice(&value.to_le_bytes());
                patches.push_str(&format!("{at} {value:x}\n"));
                ours.push((format!("{at} {value:x}"), crates_read(&damaged)));
            }
        }
        let path = path.display().to_string();
        let theirs = reports(&reader(&["--patch", &path], &patches));
        assert_eq!(theirs.len(), ours.len(), "{golden}");
        for ((name, lines), (patch, ours)) in theirs.iter().zip(&ours) {
            assert_eq!(&verdict(lines), ours, "{golden} with {patch} ({name})");
        }
        cases += ours.len();
    }
    // Every golden file damaged in more than a few places.
    assert!(cases > 1000, "{cases} damaged files");
}

/// The archive golden files, and the boxes their grids are read through by region
/// (docs/formats.md §8.6): about a leaf and the lower tile beside it, about the other leaf,
/// within the root tile, meeting nothing, and just past a leaf's last voxel and just short
/// of its first.
#[cfg(feature = "archive")]
const ARCHIVES: [&str; 2] = ["archive_v2_grids.bin", "archive_v2_quantised.bin"];
#[cfg(feature = "archive")]
const BOXES: [[i32; 6]; 6] = [
    [-12, -4, -4, 3, 11, 11],
    [4090, -2, 6, 4105, 9, 17],
    [0, 8192, 0, 5, 8197, 5],
    [100_000, 0, 0, 100_003, 3, 3],
    [0, 0, 0, 3, 3, 3],
    [-12, 0, 0, -9, 3, 3],
];

/// The crates' region of grid `index` through `b`, printed as the reader prints it, or
/// nothing where it is refused.
#[cfg(feature = "archive")]
fn crates_region(bytes: &[u8], index: usize, b: [i32; 6]) -> Option<Vec<String>> {
    fn lines<T: Stored>(r: &brodo::archive::Region, b: [i32; 6]) -> Vec<String> {
        let view = r.view::<T>().unwrap();
        let mut out: Vec<String> =
            view.leaves().map(|(n, o)| format!("leaf {n} {} {} {}", o[0], o[1], o[2])).collect();
        for z in b[2]..=b[5] {
            for y in b[1]..=b[4] {
                for x in b[0]..=b[3] {
                    let v = view.get([x, y, z]).unwrap();
                    out.push(format!("voxel {x} {y} {z} {}", hex(v.as_bytes())));
                }
            }
        }
        out
    }
    let extent = Bounds::Box { min: [b[0], b[1], b[2]], max: [b[3], b[4], b[5]] };
    let r = region_from_bytes(bytes, index, extent, u64::MAX).ok()?;
    Some(match r.value_type().code() {
        1 => lines::<f32>(&r, b),
        2 => lines::<f64>(&r, b),
        3 => lines::<u8>(&r, b),
        4 => lines::<u32>(&r, b),
        5 => lines::<[f32; 3]>(&r, b),
        6 => lines::<[f64; 3]>(&r, b),
        code => panic!("value type {code}"),
    })
}

/// The reader's arguments for a region of grid `index` through `b`.
#[cfg(feature = "archive")]
fn region_args(index: usize, b: [i32; 6]) -> Vec<String> {
    std::iter::once("--region".to_owned())
        .chain(std::iter::once(index.to_string()))
        .chain(b.iter().map(ToString::to_string))
        .collect()
}

#[cfg(feature = "archive")]
#[test]
fn the_document_s_reader_reads_every_region_of_the_archives_as_the_crates_do() {
    for golden in ARCHIVES {
        let path = data(golden).display().to_string();
        let bytes = std::fs::read(&path).unwrap();
        for index in 0..cache_file(&bytes).unwrap().grids().len() {
            for b in BOXES {
                let mut args = region_args(index, b);
                args.push(path.clone());
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                let out = reports(&reader(&args, ""));
                let ours = crates_region(&bytes, index, b).expect("the crates read the region");
                assert_eq!(out[0].1, ours, "{golden}, grid {index}, box {b:?}");
            }
        }
    }
}

#[cfg(feature = "archive")]
#[test]
fn the_document_s_reader_accepts_and_refuses_damaged_regions_as_the_crates_do() {
    let (mut cases, mut whole_refused) = (0, 0);
    for golden in ARCHIVES {
        let path = data(golden);
        let bytes = std::fs::read(&path).unwrap();
        for (index, b) in [(0, BOXES[0]), (1, BOXES[1]), (1, BOXES[2])] {
            let mut patches = String::new();
            let mut ours = Vec::new();
            for at in damaged_words(&bytes) {
                let was = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
                for value in [0, 0xFFFF_FFFF, was ^ 1, was ^ 0x8000_0000] {
                    if value == was {
                        continue;
                    }
                    let mut damaged = bytes.clone();
                    damaged[at..at + 4].copy_from_slice(&value.to_le_bytes());
                    patches.push_str(&format!("{at} {value:x}\n"));
                    let accepted = crates_region(&damaged, index, b).is_some();
                    if accepted && cache_file(&damaged).is_err() {
                        whole_refused += 1;
                    }
                    ours.push((format!("{at} {value:x}"), accepted));
                }
            }
            let mut args = vec!["--patch".to_owned(), path.display().to_string()];
            args.extend(region_args(index, b));
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let theirs = reports(&reader(&args, &patches));
            assert_eq!(theirs.len(), ours.len(), "{golden}");
            for ((name, lines), (patch, accepted)) in theirs.iter().zip(&ours) {
                let want = if *accepted { "accepted" } else { "refused" };
                assert_eq!(verdict(lines), [want], "{golden}, grid {index}, {patch} ({name})");
            }
            cases += ours.len();
        }
    }
    // Damaged in more than a few places, some of them a whole read refuses and a region
    // answers.
    assert!(cases > 1000, "{cases} damaged files");
    assert!(whole_refused > 0, "no damage a region reads past");
}

/// An archive of one grid, its leaves at the origin and 128 voxels on in x, two lower nodes
/// of one upper node, the second lower node's first child made the first's leaf: its first
/// chunk changed and compressed again, the chunk after it moved (docs/formats.md §8.2, §8.3).
#[cfg(feature = "archive")]
fn archive_with_a_shared_leaf() -> Vec<u8> {
    use brodo::cache::CacheEntry;
    use brodo::field::Field;
    use brodo::meta::GridMeta;
    use brodo::topology::Topology;
    let empty = Topology::new();
    let mut e = empty.edit();
    e.activate([0, 0, 0]).unwrap();
    e.activate([128, 0, 0]).unwrap();
    let t = e.commit().unwrap().0;
    let meta = GridMeta::new("g", [1.0; 3], "mm").unwrap();
    let entry = CacheEntry::new(&meta, &t, &Field::new(&t, 1.0_f32)).unwrap();
    // The writer's rules refuse a shared leaf (§2.5, rule 11), a region's read among them.
    let bytes = brodo::archive::to_bytes(&[entry]).unwrap();
    let word = |b: &[u8], at: usize| u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize;
    let (table, chunks) = (word(&bytes, 40) * 32, word(&bytes, 44));
    assert_eq!(chunks, 2, "the nodes, then the leaves");
    let (raw, len) = (word(&bytes, table) * 32, word(&bytes, table + 4));
    let start = (table + 8 * chunks).next_multiple_of(32);
    let mut head = lz4_flex::block::decompress(&bytes[start..start + len], raw).unwrap();
    // The lower nodes' section, from the frozen header; the second node's first child.
    let at = word(&head, 44) * 32 + 1056 + 1024;
    assert_eq!(word(&head, at), 1);
    head[at..at + 4].fill(0);
    let packed = lz4_flex::block::compress(&head);
    let mut out = bytes[..start].to_vec();
    out[table + 4..table + 8].copy_from_slice(&u32::try_from(packed.len()).unwrap().to_le_bytes());
    out.extend(&packed);
    out.resize(out.len().next_multiple_of(32), 0);
    out.extend(&bytes[(start + len).next_multiple_of(32)..]);
    out
}

/// A region whose walk reaches a leaf twice is refused, and one reaching it once read, by
/// the document's reader as by the crates (§8.6 rule 6).
#[cfg(feature = "archive")]
#[test]
fn the_document_s_reader_refuses_a_region_of_nodes_sharing_a_leaf_as_the_crates_do() {
    let bytes = archive_with_a_shared_leaf();
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("archive_with_a_shared_leaf.bin");
    std::fs::write(&path, &bytes).unwrap();
    // Refused whichever box: the nodes break the writer's rules, not only the walk to it.
    for (b, accepted) in [([0, 0, 0, 135, 7, 7], false), ([128, 0, 0, 135, 7, 7], false)] {
        let mut args = region_args(0, b);
        args.extend(["--brief".to_owned(), path.display().to_string()]);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = reports(&reader(&args, ""));
        assert_eq!(crates_region(&bytes, 0, b).is_some(), accepted, "the crates, box {b:?}");
        let want = if accepted { "accepted" } else { "refused" };
        assert_eq!(verdict(&out[0].1), [want], "the reader, box {b:?}");
    }
}
