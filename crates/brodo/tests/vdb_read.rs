//! Format tier (TESTING.md §1) for reading OpenVDB files (docs/formats.md §9.7, ADR-0047):
//! files OpenVDB 12.0.0 and 14.0.0 wrote are read as OpenVDB's own reading gives them, as
//! `spikes/vdbread/oracle/`'s `oracle expect` listed it into `vdb_read_expect.txt`; a grid
//! brodo writes reads back the same; what §9.7 refuses is refused.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use brodo::error::VdbError;
use brodo::field::AnyField;
use brodo::field::Field;
use brodo::meta::{GridMeta, SDF_KEY, SDF_VERSION};
use brodo::topology::Topology;
use brodo::tree::TileLevel;
use brodo::vdb::{VdbEntry, VdbFile, VdbValue, from_bytes, read, to_bytes};

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
            AnyField::F32(f) => lines_of(&g.meta, &g.topology, f),
            AnyField::F64(f) => lines_of(&g.meta, &g.topology, f),
            AnyField::U8(_) | AnyField::U32(_) | AnyField::F32x3(_) | AnyField::F64x3(_) | _ => {
                panic!("a value type this test does not know")
            }
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
    let (got, want) =
        (listing_of(&from_bytes(bytes, "m", u64::MAX).unwrap()), listing_expected(expect));
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
    from_bytes(&bytes, "mm", u64::MAX).unwrap()
}

#[test]
fn a_grid_brodo_writes_reads_back_the_same() {
    let (m, t, f) = written(3.0_f32, |k| k as f32 * 0.5 - 900.0);
    let file = round_trip(&m, &t, &f);
    let (m2, t2, f2) = (&file.grids[0].meta, &file.grids[0].topology, &file.grids[0].field);
    let AnyField::F32(f2) = f2 else { panic!("not a float grid") };
    let mut want = m.clone();
    want.extra = BTreeMap::from([("class".to_owned(), "level set".to_owned())]);
    assert_eq!(m2, &want);
    assert_eq!(lines_of(m2, t2, f2), lines_of(&want, &t, &f));
    let (m, t, g) = written(-1.0_f64, |k| f64::from(k) * 0.25 + 1e-12 * f64::from(k % 7));
    let file = round_trip(&m, &t, &g);
    let AnyField::F64(g2) = &file.grids[0].field else { panic!("not a double grid") };
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
    assert!(matches!(
        from_bytes(&zip, "m", u64::MAX),
        Err(VdbError::Unsupported("zip compression"))
    ));
    let mut old = BY14.to_vec();
    old[8] = 223;
    assert!(matches!(from_bytes(&old, "m", u64::MAX), Err(VdbError::Version(223))));
    let mut longer = BY14.to_vec();
    longer.push(0);
    assert!(matches!(from_bytes(&longer, "m", u64::MAX), Err(VdbError::Malformed("file's end"))));
}

#[test]
fn every_truncation_is_refused() {
    for len in (0..BY14.len()).step_by(997) {
        assert!(
            matches!(from_bytes(&BY14[..len], "m", u64::MAX), Err(VdbError::Malformed(_))),
            "{len} bytes read"
        );
    }
}

#[test]
fn a_file_above_the_limit_is_refused_before_it_is_read() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/vdb_by14_v225.bin");
    let (length, file) = (BY14.len() as u64, from_bytes(BY14, "m", u64::MAX).unwrap());
    let refused = read(&path, "m", length - 1);
    assert!(matches!(refused, Err(VdbError::FileTooLarge { length: l, .. }) if l == length));
    let limit = length.max(charged(&file));
    assert_eq!(listing_of(&read(&path, "m", limit).unwrap()), listing_of(&file));
}

/// What reading a file counts against its limit, as ARCHITECTURE.md §4.7 states: each leaf
/// twice its values' bytes and 1024 more, each tile kept 256, each lower node 2560 and each
/// upper node 18 432; a grid skipped nothing.
fn charged(file: &VdbFile) -> u64 {
    let mut total = 0;
    for g in &file.grids {
        let t = &g.topology;
        let size = if let AnyField::F64(_) = &g.field { 8 } else { 4 };
        let floor = |o: [i32; 3], span: i32| o.map(|c| c.div_euclid(span) * span);
        let (mut lowers, mut uppers) = (BTreeSet::new(), BTreeSet::new());
        for n in t.leaf_numbers() {
            let o = t.leaf_origin(n).unwrap();
            lowers.insert(floor(o, 128));
            uppers.insert(floor(o, 4096));
        }
        for n in t.tile_numbers() {
            let (level, o) = t.tile(n).unwrap();
            if level == TileLevel::Lower {
                lowers.insert(floor(o, 128));
            }
            if level != TileLevel::Root {
                uppers.insert(floor(o, 4096));
            }
        }
        total += t.leaf_numbers().count() as u64 * (2 * 512 * size + 1024)
            + t.tile_numbers().count() as u64 * 256
            + lowers.len() as u64 * 2560
            + uppers.len() as u64 * 18_432;
    }
    total
}

/// A file is read under a limit of what it is counted to make, and refused under one byte
/// less, before it makes more: files OpenVDB wrote, compressed and not, and one brodo wrote
/// with a leaf, a tile and a node at every level.
#[test]
fn what_reading_makes_is_bounded_by_the_limit() {
    let (_, _, written) = written_file();
    for bytes in [BY12, BY14, NONE, written.as_slice()] {
        let need = charged(&from_bytes(bytes, "m", u64::MAX).unwrap());
        let refused = from_bytes(bytes, "m", need - 1);
        assert!(
            matches!(refused, Err(VdbError::FileTooLarge { length, limit })
                if limit == need - 1 && length > limit && length <= need),
            "{refused:?}"
        );
        assert_eq!(
            listing_of(&from_bytes(bytes, "m", need).unwrap()),
            listing_of(&from_bytes(bytes, "m", u64::MAX).unwrap())
        );
    }
}

/// A file longer than the limit is refused by the bytes read, whatever length it reports:
/// a named pipe reports none, as `/dev/zero` does, and a file can grow after its length is
/// read (ADR-0036).
#[cfg(unix)]
#[test]
fn a_file_longer_than_the_limit_is_refused_whatever_length_it_reports() {
    use std::io::Write;
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("vdb_longer_than_its_length.fifo");
    std::fs::remove_file(&path).unwrap_or(());
    let made = std::process::Command::new("mkfifo").arg(&path).status().unwrap();
    assert!(made.success());
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
    let writer = {
        let path = path.clone();
        std::thread::spawn(move || {
            let mut pipe = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
            // The reader stops at its limit and closes the pipe: the rest is refused.
            pipe.write_all(&vec![0_u8; 1 << 20]).unwrap_or(());
        })
    };
    let fifo = read(&path, "m", 1024).map(|_| ());
    writer.join().unwrap();
    std::fs::remove_file(&path).unwrap_or(());
    assert!(matches!(fifo, Err(VdbError::FileTooLarge { limit: 1024, .. })), "{fifo:?}");
    let zero = read(Path::new("/dev/zero"), "m", 1024).map(|_| ());
    assert!(matches!(zero, Err(VdbError::FileTooLarge { length: 1025, limit: 1024 })), "{zero:?}");
}

// What OpenVDB's files do not hold, made from them or from brodo's (TESTING.md T3): each
// test below fails on a mutant of the reader that the files above leave alive.

fn u32_at(b: &[u8], at: usize) -> usize {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize
}

fn i64_at(b: &[u8], at: usize) -> i64 {
    i64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}

fn int(v: usize) -> i64 {
    i64::try_from(v).unwrap()
}

fn size(v: i64) -> usize {
    usize::try_from(v).unwrap()
}

fn put_i64(b: &mut [u8], at: usize, v: i64) {
    b[at..at + 8].copy_from_slice(&v.to_le_bytes());
}

/// Each grid's name and where its three offsets are (docs/formats.md §9.1, §9.2).
fn descriptors(bytes: &[u8]) -> Vec<(String, usize)> {
    let mut at = 8 + 4 + 8 + 1 + 36;
    assert_eq!(u32_at(bytes, at), 0, "this test reads a file without file metadata");
    let count = u32_at(bytes, at + 4);
    at += 8;
    let mut out = Vec::new();
    for _ in 0..count {
        let len = u32_at(bytes, at);
        let name = String::from_utf8(bytes[at + 4..at + 4 + len].to_vec()).unwrap();
        at += 4 + len;
        for _ in 0..2 {
            at += 4 + u32_at(bytes, at);
        }
        out.push((name, at));
        at = size(i64_at(bytes, at + 16));
    }
    out
}

/// Where grid `name`'s offsets are, and the three of them.
fn offsets(bytes: &[u8], name: &str) -> (usize, [usize; 3]) {
    let (_, at) = descriptors(bytes).into_iter().find(|(n, _)| n == name).unwrap();
    (at, [0, 1, 2].map(|k| size(i64_at(bytes, at + 8 * k))))
}

/// `bytes` with `range` replaced by `with`, every offset at or past its end moved with it.
fn splice(bytes: &[u8], range: std::ops::Range<usize>, with: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    let delta = int(with.len()) - int(range.len());
    for (_, at) in descriptors(bytes) {
        for k in 0..3 {
            let o = i64_at(&out, at + 8 * k);
            if o >= int(range.end) {
                put_i64(&mut out, at + 8 * k, o + delta);
            }
        }
    }
    out.splice(range, with.iter().copied());
    out
}

fn malformed(bytes: &[u8]) -> &'static str {
    match from_bytes(bytes, "m", u64::MAX) {
        Err(VdbError::Malformed(what)) => what,
        other => panic!("not refused as malformed: {:?}", other.map(|f| f.skipped)),
    }
}

#[test]
fn a_grid_whose_offsets_disagree_is_refused() {
    let (at, [start, _, end]) = offsets(BY14, "sdf");
    let with = |k: usize, v: usize| {
        let mut b = BY14.to_vec();
        put_i64(&mut b, at + 8 * k, int(v));
        b
    };
    assert_eq!(malformed(&with(0, start + 1)), "grid offset");
    assert_eq!(malformed(&with(2, BY14.len() + 1)), "grid offset");
    assert_eq!(malformed(&with(1, start - 1)), "grid offset");
    assert_eq!(malformed(&with(1, start)), "topology's end");
    assert_eq!(malformed(&with(1, end + 1)), "grid offset");
    assert_eq!(malformed(&with(1, end)), "topology's end");
    let (at, [start, _, _]) = offsets(BY14, "sdf_instance");
    assert_eq!(descriptors(BY14).last().unwrap().0, "sdf_instance", "the last grid is skipped");
    let mut b = BY14.to_vec();
    put_i64(&mut b, at + 16, int(start));
    assert_eq!(malformed(&b), "file's end");
    put_i64(&mut b, at + 16, int(start) - 1);
    assert_eq!(malformed(&b), "grid offset");
}

#[test]
fn a_file_below_the_limit_is_read() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/vdb_by14_v225.bin");
    assert_eq!(
        listing_of(&read(&path, "m", u64::MAX).unwrap()),
        listing_of(&from_bytes(BY14, "m", u64::MAX).unwrap())
    );
}

/// The double grid of `written`, its root tile, and the file brodo writes of it.
fn written_file() -> (Topology, Field<f64>, Vec<u8>) {
    let (m, t, f) = written(-1.0_f64, |k| f64::from(k) * 0.25);
    let bytes = to_bytes(&[VdbEntry::new(&m, &t, &f).unwrap()]).unwrap();
    (t, f, bytes)
}

/// Where the root tile's origin is in a file of `written_file`, and its active byte.
fn root_tile(t: &Topology, f: &Field<f64>, bytes: &[u8]) -> (usize, usize) {
    let n = t.tile_numbers().find(|&n| t.tile(n).unwrap().0 == TileLevel::Root).unwrap();
    let mut pattern: Vec<u8> = t.tile(n).unwrap().1.iter().flat_map(|c| c.to_le_bytes()).collect();
    pattern.extend(f.tile(t, n).unwrap().to_le_bytes());
    pattern.push(1);
    let at = bytes.windows(pattern.len()).position(|w| w == pattern).unwrap();
    (at, at + pattern.len() - 1)
}

#[test]
fn an_inactive_root_tile_of_another_value_is_kept() {
    let (t, f, mut bytes) = written_file();
    let (_, active) = root_tile(&t, &f, &bytes);
    bytes[active] = 0;
    let file = from_bytes(&bytes, "mm", u64::MAX).unwrap();
    let AnyField::F64(g) = &file.grids[0].field else { panic!("not a double grid") };
    assert_eq!(
        lines_of(&file.grids[0].meta, &file.grids[0].topology, g),
        lines_of(&file.grids[0].meta, &t, &f)
    );
}

#[test]
fn a_root_origin_off_its_edge_and_a_map_not_affine_are_refused() {
    let (t, f, bytes) = written_file();
    let (origin, _) = root_tile(&t, &f, &bytes);
    let mut off = bytes.clone();
    off[origin] = 1;
    assert_eq!(malformed(&off), "root's origin");
    let at = bytes.windows(9).position(|w| w == b"AffineMap").unwrap() + 9 + 15 * 8;
    assert_eq!(
        f64::from_le_bytes(bytes[at..at + 8].try_into().unwrap()).to_bits(),
        1.0_f64.to_bits()
    );
    let mut map = bytes;
    map[at..at + 8].copy_from_slice(&2.0_f64.to_le_bytes());
    assert_eq!(malformed(&map), "AffineMap");
}

#[test]
fn an_instance_of_a_double_grid_is_skipped() {
    let (_, _, bytes) = written_file();
    let (_, at) = descriptors(&bytes)[0].clone();
    let mut b = splice(&bytes, at..at, b"x");
    b[at - 4..at].copy_from_slice(&1_u32.to_le_bytes());
    let file = from_bytes(&b, "mm", u64::MAX).unwrap();
    assert!(file.grids.is_empty());
    assert_eq!(file.skipped, ["g"]);
}

#[test]
fn indicators_openvdb_does_not_write_here_read_as_section_9_8_says() {
    // Without active-mask every value is stored, whatever the indicator.
    let (_, [_, buffers, _]) = offsets(NONE, "inactive");
    let mut none = NONE.to_vec();
    assert_eq!(none[buffers + 64], 6);
    none[buffers + 64] = 0;
    same_listing(&none, NONE_EXPECT);
    // Indicator 1: the fog's first leaf, its inactive places minus the background.
    let (_, [_, buffers, _]) = offsets(BY14, "fog");
    let mut fog = BY14.to_vec();
    assert_eq!(fog[buffers + 64], 0);
    fog[buffers + 64] = 1;
    let negated = |bytes: &[u8]| {
        let file = from_bytes(bytes, "m", u64::MAX).unwrap();
        let g = file.grids.iter().find(|g| g.meta.name == "fog").unwrap();
        let AnyField::F64(f) = &g.field else { panic!("not a double grid") };
        let t = &g.topology;
        let bits = (-f.background()).to_bits();
        t.leaf_numbers().flat_map(|n| f.leaf(t, n).unwrap()).filter(|v| v.to_bits() == bits).count()
    };
    assert_eq!(negated(BY14), 0);
    assert!(negated(&fog) > 0);
}

const LZ4: u8 = 1 << 5;
const SHUFFLE: u8 = 1;
const NO_SPLIT: u8 = 0x10;

/// Each Blosc chunk OpenVDB stored as it is (memcpy) within `within`: where its `i64`
/// count is, and the bytes of values it holds.
fn memcpy_chunks(bytes: &[u8], within: std::ops::Range<usize>) -> Vec<(usize, Vec<u8>)> {
    (within.start..within.end - 24)
        .filter(|&p| {
            let n = i64_at(bytes, p);
            n >= 16
                && bytes[p + 8] == 2
                && bytes[p + 10] & 2 != 0
                && int(u32_at(bytes, p + 20)) == n
                && int(u32_at(bytes, p + 12)) + 16 == n
        })
        .map(|p| (p, bytes[p + 24..p + 8 + size(i64_at(bytes, p))].to_vec()))
        .collect()
}

/// A Blosc chunk of `raw`'s bytes in one block of one stream, stored as it is.
fn chunk(raw: &[u8], flags: u8, size: u8, block: usize) -> Vec<u8> {
    let mut c = vec![2, 1, flags, size];
    for w in [raw.len(), block, 16 + 4 + 4 + raw.len()] {
        c.extend(u32::try_from(w).unwrap().to_le_bytes());
    }
    c.extend(20_i32.to_le_bytes());
    c.extend(i32::try_from(raw.len()).unwrap().to_le_bytes());
    c.extend(raw);
    c
}

/// Byte b of each of `raw`'s whole values of `size` bytes moved to stream b.
fn shuffle(raw: &[u8], size: usize) -> Vec<u8> {
    let m = raw.len() / size;
    let mut out = raw.to_vec();
    for j in 0..m {
        for b in 0..size {
            out[b * m + j] = raw[j * size + b];
        }
    }
    out
}

/// BY14 with the stored values at `p`, a chunk of their bytes, stored as `with` instead.
fn stored_as(p: usize, n: usize, with: &[u8]) -> Vec<u8> {
    splice(BY14, p..p + 8 + n, with)
}

fn counted(chunk: &[u8]) -> Vec<u8> {
    let mut out = int(chunk.len()).to_le_bytes().to_vec();
    out.extend(chunk);
    out
}

/// A chunk of the inactive grid's values that a shuffle changes, and where it is.
fn a_chunk() -> (usize, Vec<u8>) {
    let (_, [start, _, end]) = offsets(BY14, "inactive");
    let (p, raw) = memcpy_chunks(BY14, start..end)
        .into_iter()
        .find(|(_, raw)| {
            (8..255).contains(&raw.len()) && shuffle(raw, 4) != *raw && shuffle(raw, 2) != *raw
        })
        .unwrap();
    (p, raw)
}

#[test]
fn values_stored_in_other_ways_read_the_same() {
    let (p, raw) = a_chunk();
    let n = 16 + raw.len();
    let mut plain = (-int(raw.len())).to_le_bytes().to_vec();
    plain.extend(&raw);
    same_listing(&stored_as(p, n, &plain), EXPECT);
    let len = raw.len();
    for c in [
        chunk(&raw, LZ4 | NO_SPLIT, 4, len),
        chunk(&raw, LZ4, 4, len + 1),
        chunk(&shuffle(&raw, 2), LZ4 | SHUFFLE | NO_SPLIT, 2, len),
        chunk(&raw, LZ4 | SHUFFLE | NO_SPLIT, 255, len),
    ] {
        same_listing(&stored_as(p, n, &counted(&c)), EXPECT);
    }
    let (q, _) =
        memcpy_chunks(BY14, 0..BY14.len()).into_iter().find(|(_, r)| r.is_empty()).unwrap();
    same_listing(&stored_as(q, 16, &0_i64.to_le_bytes()), EXPECT);
}

#[test]
fn blosc_chunks_beyond_section_9_8_are_refused() {
    let (p, raw) = a_chunk();
    let (n, len) = (16 + raw.len(), raw.len());
    let mut long = BY14.to_vec();
    long[p + 20..p + 24].copy_from_slice(&(u32::try_from(n).unwrap() + 1).to_le_bytes());
    assert_eq!(malformed(&long), "Blosc chunk");
    let unsupported = |c: Vec<u8>| match from_bytes(&stored_as(p, n, &counted(&c)), "m", u64::MAX) {
        Err(VdbError::Unsupported(what)) => what,
        other => panic!("not refused as unsupported: {:?}", other.map(|f| f.skipped)),
    };
    assert_eq!(
        unsupported(chunk(&raw, NO_SPLIT, 4, len)),
        "a Blosc codec other than LZ4 with byte shuffle"
    );
    assert_eq!(
        unsupported(chunk(&raw, LZ4 | NO_SPLIT | 4, 4, len)),
        "a Blosc codec other than LZ4 with byte shuffle"
    );
    assert_eq!(
        unsupported(chunk(&raw, LZ4 | NO_SPLIT, 4, len - 1)),
        "a Blosc chunk of more than one block"
    );
    assert_eq!(malformed(&stored_as(p, n, &counted(&chunk(&raw, LZ4, 0, len)))), "Blosc chunk");
    // A chunk stating no bytes is not one of more than one block: it is malformed.
    let mut none = chunk(&raw, LZ4 | NO_SPLIT, 4, len);
    none[4..8].fill(0);
    assert_eq!(malformed(&stored_as(p, n, &counted(&none))), "Blosc chunk");
}

/// `c` with `with` put at `at` and its own byte count kept true.
fn chunk_with(c: &[u8], at: usize, with: &[u8]) -> Vec<u8> {
    let mut out = c.to_vec();
    out.splice(at..at, with.iter().copied());
    let n = u32::try_from(out.len()).unwrap();
    out[12..16].copy_from_slice(&n.to_le_bytes());
    out
}

#[test]
fn a_blosc_block_off_its_place_or_with_bytes_after_it_is_refused() {
    let (p, raw) = a_chunk();
    let (n, len) = (16 + raw.len(), raw.len());
    let refused = |c: &[u8]| malformed(&stored_as(p, n, &counted(c)));
    let lz4 = chunk(&raw, LZ4 | NO_SPLIT, 4, len);
    let mut later = chunk_with(&lz4, 20, &[0; 4]);
    later[16..20].copy_from_slice(&24_i32.to_le_bytes());
    assert_eq!(refused(&later), "Blosc chunk");
    let mut earlier = lz4.clone();
    earlier[16..20].copy_from_slice(&16_i32.to_le_bytes());
    assert_eq!(refused(&earlier), "Blosc chunk");
    assert_eq!(refused(&chunk_with(&lz4, lz4.len(), &[0; 3])), "Blosc chunk");
    let memcpy = chunk_with(&BY14[p + 8..p + 8 + n], n, &[0]);
    assert_eq!(refused(&memcpy), "Blosc chunk");
}
