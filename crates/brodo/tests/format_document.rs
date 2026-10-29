//! Format tier (TESTING.md §1) for the format document (ADR-0037): `scripts/audit/formats.py`,
//! a reader written from `docs/formats.md` alone, reads every golden file as the crates
//! read it, value for value, and accepts or refuses each damaged copy of them as the
//! crates do. A difference means the document leaves out or misstates what a reader needs.

use std::path::{Path, PathBuf};
use std::process::Command;

use brodo::cache::CacheFile;
use brodo::field::Stored;
use brodo::hermite::HermiteView;
use brodo::meta::Bounds;
use brodo::tree::TileLevel;
use brodo::view::{GridView, ViewLocation};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

/// The golden files, the archive's where the crate reads it.
const GOLDEN: &[&str] = &[
    "frozen_v1_f32.bin",
    "frozen_v1_f64x3.bin",
    "cache_v1.bin",
    "hermite_v1.bin",
    #[cfg(feature = "archive")]
    "archive_v1.bin",
    #[cfg(feature = "archive")]
    "archive_v1_quantised.bin",
];

/// A cache file as the crates read the bytes: a cache file's, or an archive's decompressed.
fn cache_file(bytes: &[u8]) -> Result<CacheFile, brodo::error::CacheError> {
    #[cfg(feature = "archive")]
    if bytes.starts_with(b"BRDA") {
        return brodo::archive::from_bytes(bytes, u64::MAX);
    }
    CacheFile::from_bytes(bytes)
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
            same_values(&file.view::<$t>(i).unwrap(), lines)
        };
    }
    let m = file.meta(i).unwrap();
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
                let ours = match file.bounds(i).unwrap() {
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
    match file.value_type(i).unwrap() {
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
    let mut sets: Vec<String> = (0..file.grid_count())
        .filter_map(|i| file.meta(i).unwrap().extra.get("hermite.set").cloned())
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
        assert_eq!(grids.len(), file.grid_count(), "{name}: grid count");
        for (i, (code, lines)) in grids.iter().enumerate() {
            assert_eq!(*code, file.value_type(i).unwrap(), "{name}: grid {i}'s type");
            same_cache_grid(&file, i, lines);
        }
        let hermite: Vec<String> =
            lines.iter().filter(|l| l.starts_with("hermite ")).cloned().collect();
        assert_eq!(verdict(&hermite), crates_read(&bytes)[1..], "{name}: Hermite sets");
    }
}

/// The Hermite golden file, its last grid, the z groups, holding one tile value more,
/// reached by no node: its frozen buffer grown by a block (docs/formats.md §2.1, §3.2).
fn hermite_with_a_tile_more() -> Vec<u8> {
    let mut b = std::fs::read(data("hermite_v1.bin")).unwrap();
    let word = |b: &[u8], at: usize| u32::from_le_bytes(b[at..at + 4].try_into().unwrap());
    let put = |b: &mut [u8], at: usize, v: u32| b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    let entry = 32 * word(&b, 8) as usize;
    let frozen = 32 * word(&b, entry + 8) as usize;
    let blocks = word(&b, frozen + 56);
    assert_eq!(word(&b, frozen + 32), 0, "a grid with no tile");
    put(&mut b, frozen + 32, 1);
    put(&mut b, frozen + 52, blocks);
    put(&mut b, frozen + 56, blocks + 1);
    put(&mut b, entry + 12, blocks + 1);
    b.extend([0; 32]);
    b
}

#[test]
fn the_document_s_reader_refuses_a_hermite_field_holding_values_no_node_reaches() {
    let bytes = hermite_with_a_tile_more();
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hermite_with_a_tile_more.bin");
    std::fs::write(&path, &bytes).unwrap();
    let out = reports(&reader(&[&path.display().to_string()], ""));
    let hermite: Vec<String> =
        out[0].1.iter().filter(|l| l.starts_with("hermite ")).cloned().collect();
    let ours = crates_read(&bytes);
    assert!(ours[1].ends_with(" refused"), "{ours:?}");
    assert_eq!(verdict(&hermite), ours[1..]);
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
