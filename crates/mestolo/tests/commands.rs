//! Format tier (TESTING.md §1) for mestolo (ADR-0050): each command's output from brodo's
//! committed files held byte for byte to a committed golden file, and the program run for
//! its command line, its exit status and its refusal to replace a file.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command as Process;

use brodo::cache::{CacheEntry, CacheFile};
use brodo::field::{Field, ValueType};
use brodo::kind::FileKind;
use brodo::meta::{GridMeta, SDF_KEY, SDF_VERSION};
use brodo::slice::Axis;
use brodo::topology::Topology;
use brodo::tree::TileLevel;
use mestolo::args::{Command, DEFAULT_LIMIT, Format, Input, Output, parse};
use mestolo::error::MestoloError;
use mestolo::info::extremes;

const INFO_CACHE: &str = include_str!("data/info_cache.txt");
const INFO_VDB: &str = include_str!("data/info_vdb.txt");
const CONTROL: &str = include_str!("data/info_control.txt");
const SLICE: &[u8] = include_bytes!("data/slice_sdf_z12.png");
const VDB: &[u8] = include_bytes!("data/convert_density.vdb");
const VTU: &[u8] = include_bytes!("data/convert_density.vtu");

/// One of brodo's committed test files.
fn brodo_file(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../brodo/tests/data").join(name)
}

/// A path for an output, none there.
fn fresh(name: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::remove_file(&path).unwrap_or(());
    path
}

fn args(words: &[&str]) -> Vec<OsString> {
    words.iter().map(OsString::from).collect()
}

fn info(name: &str) -> String {
    let mut out = Vec::new();
    mestolo::run(&parse(args(&["info", brodo_file(name).to_str().unwrap()])).unwrap(), &mut out)
        .unwrap();
    String::from_utf8(out).unwrap()
}

/// Runs `words` and gives the file written at `out`.
fn written(words: &[&str], out: &Path) -> Vec<u8> {
    let mut all = args(words);
    all.extend([OsString::from("-o"), out.as_os_str().to_owned()]);
    mestolo::run(&parse(all).unwrap(), &mut Vec::new()).unwrap();
    std::fs::read(out).unwrap()
}

#[test]
fn info_writes_what_a_cache_file_an_archive_and_a_vdb_hold() {
    // The cache file holds an entry of bytes beside the grids, listed by its name and length.
    let cache = INFO_CACHE.replacen("grids: 2\n", "grids: 2\nbytes: \"readme\", 11 bytes\n", 1);
    assert_eq!(info("cache_v2.bin"), cache);
    let archive = INFO_CACHE.replacen("kind: cache file", "kind: archive", 1);
    assert_eq!(info("archive_v2_grids.bin"), archive);
    assert_eq!(info("vdb_by14_v225.bin"), INFO_VDB);
}

#[test]
fn slice_draws_a_level_sets_plane_as_png() {
    let vdb = brodo_file("vdb_by14_v225.bin");
    let out = fresh("slice.png");
    let words = ["slice", vdb.to_str().unwrap(), "--grid", "sdf", "--axis", "z", "--at", "12"];
    assert_eq!(written(&words, &out), SLICE);
}

#[test]
fn convert_writes_a_grid_as_vdb_and_as_vtu() {
    let cache = brodo_file("cache_v2.bin");
    let path = cache.to_str().unwrap();
    assert_eq!(written(&["convert", path, "--grid", "density"], &fresh("c.vdb")), VDB);
    assert_eq!(written(&["convert", path, "--grid", "density"], &fresh("c.vtu")), VTU);
}

#[test]
fn what_a_format_does_not_hold_is_refused_by_name() {
    let cache = brodo_file("cache_v2.bin");
    let run = |words: &[&str], out: &str| {
        let mut all = args(words);
        all.extend([OsString::from("-o"), fresh(out).into_os_string()]);
        mestolo::run(&parse(all).unwrap(), &mut Vec::new())
    };
    let p = cache.to_str().unwrap();
    assert!(matches!(
        run(&["convert", p], "all.vdb"),
        Err(MestoloError::NotWritten(grids)) if grids == [("velocity".to_owned(), ValueType::F32x3)]
    ));
    assert!(matches!(run(&["convert", p], "all.vtu"), Err(MestoloError::WhichGrid(2))));
    assert!(matches!(
        run(&["convert", p, "--grid", "density"], "c.png"),
        Err(MestoloError::OutputKind(_))
    ));
    assert!(matches!(
        run(&["slice", p, "--grid", "velocity", "--axis", "x", "--at", "0"], "v.png"),
        Err(MestoloError::NotScalar(ValueType::F32x3))
    ));
    assert!(matches!(
        run(&["slice", p, "--grid", "none", "--axis", "x", "--at", "0"], "n.png"),
        Err(MestoloError::NoGrid(n)) if n == "none"
    ));
}

/// `--skip-unsupported` writes as `.vdb` the grids it holds, as `--grid` writes the one, and
/// names each it leaves out; a conversion leaving none is refused as without it.
#[test]
fn convert_skips_and_names_the_grids_vdb_does_not_hold_when_asked() {
    let cache = brodo_file("cache_v2.bin");
    let p = cache.to_str().unwrap();
    let run = |words: &[&str], out: &Path| {
        let mut all = args(words);
        all.extend([OsString::from("-o"), out.as_os_str().to_owned()]);
        let mut said = Vec::new();
        mestolo::run(&parse(all).unwrap(), &mut said).map(|()| String::from_utf8(said).unwrap())
    };
    let out = fresh("skipped.vdb");
    let said = run(&["convert", p, "--skip-unsupported"], &out).map_err(|e| e.to_string());
    assert_eq!(said.as_deref(), Ok("skipped \"velocity\": .vdb does not hold [f32; 3]\n"));
    assert_eq!(std::fs::read(&out).unwrap(), VDB);
    let only = run(&["convert", p, "--grid", "velocity", "--skip-unsupported"], &fresh("v.vdb"));
    assert!(matches!(
        only,
        Err(MestoloError::NotWritten(grids)) if grids == [("velocity".to_owned(), ValueType::F32x3)]
    ));
    let twice =
        parse(args(&["convert", p, "-o", "x.vdb", "--skip-unsupported", "--skip-unsupported"]));
    assert!(matches!(twice, Err(MestoloError::Repeated(o)) if o == "--skip-unsupported"));
    let info = parse(args(&["info", p, "--skip-unsupported"]));
    assert!(matches!(info, Err(MestoloError::UnknownOption(o)) if o == "--skip-unsupported"));
}

#[test]
fn a_file_of_no_kind_read_and_a_bare_grid_are_refused() {
    let run =
        |name: &str| mestolo::run(&Command::Info(Input::new(brodo_file(name))), &mut Vec::new());
    assert!(matches!(run("frozen_v2_f32.bin"), Err(MestoloError::BareGrid(_))));
    assert!(matches!(run("slice_sphere.png"), Err(MestoloError::UnknownKind(_))));
}

#[test]
fn the_limit_bounds_each_kind_of_file() {
    for name in ["cache_v2.bin", "archive_v2_grids.bin", "vdb_by14_v225.bin"] {
        let words = ["info", "--limit", "1024"];
        let mut all = args(&words);
        all.push(brodo_file(name).into_os_string());
        let refused = mestolo::run(&parse(all).unwrap(), &mut Vec::new());
        assert!(matches!(refused, Err(MestoloError::Cache(_) | MestoloError::Vdb(_))), "{name}");
    }
}

/// `help <command>`, `-h` or `--help` and a command's `-h` or `--help` before `--` write its
/// usage, which names its options; of no command or another, the whole usage; `--version` and
/// `-V` the version.
#[test]
fn help_writes_a_commands_usage_and_version_the_version() {
    let text = |words: &[&str]| {
        let mut out = Vec::new();
        mestolo::run(&parse(args(words)).unwrap(), &mut out).unwrap();
        String::from_utf8(out).unwrap()
    };
    for (name, options) in [
        ("info", &["--limit"][..]),
        ("slice", &["--axis", "--at", "-o", "--grid", "--limit", "--force"]),
        ("convert", &["-o", "--as", "--grid", "--limit", "--force", "--skip-unsupported"]),
    ] {
        let usage = text(&["help", name]);
        assert!(usage.starts_with(&format!("usage: mestolo {name} <file>")), "{usage}");
        assert!(options.iter().all(|o| usage.contains(&format!(" {o} "))), "{usage}");
        for words in [&[name, "f", "-h"][..], &[name, "--help", "f", "--limit", "x"]] {
            let on = Ok(Command::HelpOn(name.to_owned()));
            assert_eq!(parse(args(words)).map_err(|e| e.to_string()), on);
            assert_eq!(text(words), usage);
        }
    }
    for words in [&["help"][..], &["help", "look"], &["-h"], &["--help", "--help"]] {
        assert_eq!(text(words), mestolo::USAGE);
    }
    assert_eq!(text(&["--help", "info"]), text(&["help", "info"]));
    let input = parse(args(&["info", "--", "--help"]));
    assert!(matches!(input, Ok(Command::Info(i)) if i.path == Path::new("--help")));
    let version = format!("mestolo {}\n", env!("CARGO_PKG_VERSION"));
    assert_eq!((text(&["--version"]), text(&["-V"])), (version.clone(), version));
}

#[test]
fn the_command_line_is_parsed_and_what_it_does_not_take_named() {
    let slice = parse(args(&["slice", "f", "--axis", "y", "--at", "-3", "-o", "s.png", "--force"]));
    let Ok(Command::Slice { input, grid: None, at: -3, output, .. }) = slice else {
        panic!("{slice:?}")
    };
    assert_eq!((input.path, input.limit), (PathBuf::from("f"), DEFAULT_LIMIT));
    assert_eq!((output.path, output.force), (PathBuf::from("s.png"), true));
    assert_eq!(parse(args(&["help"])).unwrap(), Command::Help);
    let refused = |words: &[&str]| parse(args(words)).unwrap_err();
    assert!(matches!(parse(args(&[])), Err(MestoloError::NoCommand)));
    for (word, format) in [
        ("vdb", Format::Vdb),
        ("vtu", Format::Vtu),
        ("cache", Format::Cache),
        ("archive", Format::Archive),
    ] {
        let convert = parse(args(&["convert", "f", "-o", "o", "--as", word]));
        assert!(
            matches!(&convert, Ok(Command::Convert { format: Some(f), .. }) if *f == format),
            "{word}: {convert:?}"
        );
    }
    assert!(matches!(
        refused(&["convert", "f", "-o", "o", "--as", "png"]),
        MestoloError::BadValue { option: "--as", .. }
    ));
    assert!(
        matches!(refused(&["info", "f", "--as", "cache"]), MestoloError::UnknownOption(o) if o == "--as")
    );
    assert!(matches!(refused(&["look"]), MestoloError::UnknownCommand(c) if c == "look"));
    assert!(
        matches!(refused(&["info", "f", "-o", "x"]), MestoloError::UnknownOption(o) if o == "-o")
    );
    assert!(matches!(refused(&["info", "f", "g"]), MestoloError::Extra(e) if e == "g"));
    assert!(matches!(refused(&["info", "--limit"]), MestoloError::Missing("--limit")));
    assert!(matches!(
        refused(&["info", "f", "--limit", "1", "--limit", "2"]),
        MestoloError::Repeated(_)
    ));
    let twice = refused(&["convert", "f", "-o", "o", "--force", "--force"]);
    assert!(matches!(twice, MestoloError::Repeated(r) if r == "--force"));
    assert!(matches!(
        refused(&["slice", "f", "--axis", "w", "--at", "0", "-o", "o"]),
        MestoloError::BadValue { option: "--axis", .. }
    ));
    assert!(matches!(refused(&["convert", "f"]), MestoloError::Missing("-o")));
    let after = parse(args(&["info", "--", "-f"])).unwrap();
    assert_eq!(after, Command::Info(Input::new(PathBuf::from("-f"))));
}

/// The program refuses a file at its output's path, writes it by `--force`, and exits 2
/// for a command line refused and 1 for a command failing, the error on standard error.
#[test]
fn the_program_refuses_to_replace_a_file_but_by_force() {
    let exe = env!("CARGO_BIN_EXE_mestolo");
    let cache = brodo_file("cache_v2.bin");
    let out = fresh("forced.vdb");
    std::fs::write(&out, b"kept").unwrap();
    let run = |extra: &[&str]| {
        Process::new(exe)
            .args(["convert", cache.to_str().unwrap(), "--grid", "density", "-o"])
            .arg(&out)
            .args(extra)
            .output()
            .unwrap()
    };
    let refused = run(&[]);
    assert_eq!(refused.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&refused.stderr).contains("give --force"));
    assert_eq!(std::fs::read(&out).unwrap(), b"kept");
    assert_eq!(run(&["--force"]).status.code(), Some(0));
    assert_eq!(std::fs::read(&out).unwrap(), VDB);
    let usage = Process::new(exe).arg("look").output().unwrap();
    assert_eq!(usage.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&usage.stderr).contains("usage: mestolo"));
    let help = Process::new(exe).arg("help").output().unwrap();
    assert_eq!((help.status.code(), help.stdout.as_slice()), (Some(0), mestolo::USAGE.as_bytes()));
    let none = Process::new(exe).output().unwrap();
    assert_eq!((none.status.code(), none.stdout.as_slice()), (Some(2), &b""[..]));
    let said = String::from_utf8_lossy(&none.stderr);
    assert!(said.starts_with("mestolo: no command is given") && said.contains(mestolo::USAGE));
}

#[test]
fn an_output_is_refused_before_its_input_is_read() {
    let out = fresh("there.png");
    std::fs::write(&out, b"kept").unwrap();
    let output = Output::new(out.clone(), false);
    let command = Command::Convert {
        input: Input::new(PathBuf::from("/no/such/file")),
        grid: None,
        format: None,
        output,
    };
    assert!(matches!(mestolo::run(&command, &mut Vec::new()), Err(MestoloError::Exists(_))));
}

/// A cache file of one grid, written to a fresh path.
fn cache_file(name: &str, meta: &GridMeta, t: &Topology, f: &Field<f32>) -> PathBuf {
    let path = fresh(name);
    brodo::cache::write(&path, &[CacheEntry::new(meta, t, f).unwrap()]).unwrap();
    path
}

/// A file's strings, its control characters among them, are written escaped, so that none
/// moves the terminal or makes a line of its own.
#[test]
fn info_escapes_every_string_a_file_holds() {
    let empty = Topology::new();
    let mut e = empty.edit();
    e.activate([0, 0, 0]).unwrap();
    let t = e.commit().unwrap().0;
    let f = Field::new(&t, 0.5_f32);
    let mut m = GridMeta::new("g\u{1b}[2J", [1.0; 3], "m\nkind: fake").unwrap();
    m.extra.insert("key\u{7}".to_owned(), "line\r\n  least: 0\u{202e}".to_owned());
    let path = cache_file("control.cache", &m, &t, &f);
    let mut out = Vec::new();
    mestolo::run(&Command::Info(Input::new(path)), &mut out).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert_eq!(text, CONTROL);
    assert!(!text.chars().any(|c| c.is_control() && c != '\n'));
}

/// A grid of 4095 tiles of a lower node's size, each counted at 256 bytes to thaw, and one
/// of 64 leaves of `f32`, each its 2048 bytes of values and 1024 more: each read under a
/// limit of its count, and refused under one byte less.
#[test]
fn thawing_is_counted_against_the_limit() {
    let empty = Topology::new();
    let (mut tiles, mut leaves) = (empty.edit(), empty.edit());
    for p in 1..4096 {
        tiles.add_tile(TileLevel::Lower, [p % 16 * 8, p / 16 % 16 * 8, p / 256 * 8]).unwrap();
    }
    for p in 0..64 {
        leaves.activate([p % 4 * 8, p / 4 % 4 * 8, p / 16 * 8]).unwrap();
    }
    let meta = GridMeta::new("g", [1.0; 3], "m").unwrap();
    for (name, edit, need) in [("tiles", tiles, 4095 * 256), ("leaves", leaves, 64 * 3072)] {
        let t = edit.commit().unwrap().0;
        let path = cache_file(&format!("{name}.cache"), &meta, &t, &Field::new(&t, 1.0));
        assert!(std::fs::metadata(&path).unwrap().len() < need, "{name}");
        let run = |limit| {
            let mut input = Input::new(path.clone());
            input.limit = limit;
            mestolo::run(&Command::Info(input), &mut Vec::new())
        };
        assert!(run(need).is_ok(), "{name}");
        assert!(
            matches!(
                run(need - 1),
                Err(MestoloError::TooMuchToThaw { counted, limit }) if counted == need && limit == need - 1
            ),
            "{name}"
        );
    }
}

/// Of equal values, 0 and −0 here, the least and the greatest are the first in leaf order.
#[test]
fn of_equal_extremes_the_first_is_written() {
    let empty = Topology::new();
    let mut e = empty.edit();
    e.activate([0, 0, 0]).unwrap();
    let t = e.commit().unwrap().0;
    let n = t.leaf_numbers().next().unwrap();
    let mut f = Field::new(&t, 0.5_f32);
    let mut set = |values: [f32; 2], rest: f32| {
        let leaf = f.leaf_mut(&t, n).unwrap();
        leaf.fill(rest);
        leaf[..2].copy_from_slice(&values);
        extremes(&t, &f).unwrap()
    };
    let (least, _) = set([0.0, -0.0], 1.0);
    assert_eq!(least[0].map(f32::to_bits), Some(0.0_f32.to_bits()));
    let (least, _) = set([-0.0, 0.0], 1.0);
    assert_eq!(least[0].map(f32::to_bits), Some((-0.0_f32).to_bits()));
    let (_, most) = set([-0.0, 0.0], -1.0);
    assert_eq!(most[0].map(f32::to_bits), Some((-0.0_f32).to_bits()));
    let (_, most) = set([0.0, -0.0], -1.0);
    assert_eq!(most[0].map(f32::to_bits), Some(0.0_f32.to_bits()));
}

#[test]
fn a_grid_is_found_by_its_name() {
    let loaded = mestolo::load::load(&Input::new(brodo_file("cache_v2.bin"))).unwrap();
    for name in ["density", "velocity"] {
        assert_eq!(loaded.grid(Some(name)).unwrap().meta.name, name);
    }
}

/// The sdf grid of OpenVDB's file spans x from 8 to 23 and z from 0 to 23: a plane at
/// either end is drawn, and one a voxel beyond either is refused, naming the span.
#[test]
fn a_slice_beyond_the_grid_is_refused_and_one_at_its_edge_drawn() {
    let vdb = brodo_file("vdb_by14_v225.bin");
    let run = |axis: &str, at: i32| {
        let out = fresh("edge.png");
        let at = at.to_string();
        let words = ["slice", vdb.to_str().unwrap(), "--grid", "sdf", "--axis", axis, "--at", &at];
        let mut all = args(&words);
        all.extend([OsString::from("-o"), out.into_os_string()]);
        mestolo::run(&parse(all).unwrap(), &mut Vec::new())
    };
    for (axis, a, min, max) in [("z", Axis::Z, 0, 23), ("x", Axis::X, 8, 23)] {
        assert!(run(axis, min).is_ok() && run(axis, max).is_ok(), "{axis}");
        for at in [min - 1, max + 1] {
            let refused = run(axis, at);
            assert!(
                matches!(&refused, Err(MestoloError::OutsideGrid { grid, axis: got, at: a2, min: m0, max: m1 })
                    if grid == "sdf" && *got == a && *a2 == at && *m0 == min && *m1 == max),
                "{axis} {at}: {refused:?}"
            );
            let said = refused.unwrap_err().to_string();
            assert!(said.contains(&format!(" along {axis}, ")), "{said}");
        }
    }
}

/// A cache file of three grids, two of types `.vdb` does not hold.
#[test]
fn every_grid_vdb_does_not_hold_is_named() {
    let empty = Topology::new();
    let mut e = empty.edit();
    e.activate([0, 0, 0]).unwrap();
    let t = e.commit().unwrap().0;
    let meta = |name: &str| GridMeta::new(name, [1.0; 3], "m").unwrap();
    let path = fresh("three.cache");
    let entries = [
        CacheEntry::new(&meta("labels"), &t, &Field::new(&t, 1_u8)).unwrap(),
        CacheEntry::new(&meta("density"), &t, &Field::new(&t, 0.5_f32)).unwrap(),
        CacheEntry::new(&meta("flow"), &t, &Field::new(&t, [0.0_f64; 3])).unwrap(),
    ];
    brodo::cache::write(&path, &entries).unwrap();
    let mut all = args(&["convert", path.to_str().unwrap(), "-o"]);
    all.push(fresh("three.vdb").into_os_string());
    let refused = mestolo::run(&parse(all).unwrap(), &mut Vec::new());
    let want = vec![("labels".to_owned(), ValueType::U8), ("flow".to_owned(), ValueType::F64x3)];
    assert!(
        matches!(&refused, Err(MestoloError::NotWritten(grids)) if *grids == want),
        "{refused:?}"
    );
    let said = refused.unwrap_err().to_string();
    assert!(said.contains("\"labels\"") && said.contains("\"flow\""), "{said}");
}

/// Each entry of a file as bytes: a grid's metadata, bounds, type and frozen buffer, or an
/// entry of bytes' name, pairs and bytes, the grids first.
fn entries_of(file: &CacheFile) -> Vec<Vec<u8>> {
    let copies: Vec<CacheEntry> = file
        .grids()
        .into_iter()
        .map(CacheEntry::copy)
        .chain(file.bytes_entries().into_iter().map(CacheEntry::copy_bytes))
        .collect();
    copies.iter().map(|c| brodo::cache::to_bytes(std::slice::from_ref(c)).unwrap()).collect()
}

/// Runs `convert` on `input` with `--as` and the extra words, giving the file written.
fn converted(input: &Path, extra: &[&str], out: &str) -> PathBuf {
    let out = fresh(out);
    let mut all = args(&["convert", input.to_str().unwrap()]);
    all.extend(args(extra));
    all.extend([OsString::from("-o"), out.clone().into_os_string()]);
    mestolo::run(&parse(all).unwrap(), &mut Vec::new()).unwrap();
    out
}

#[test]
fn convert_writes_a_cache_file_and_an_archive_holding_the_entries_as_they_were() {
    let cache = brodo_file("cache_v2.bin");
    let archive = brodo_file("archive_v2_grids.bin");
    let original = entries_of(&brodo::cache::read(&cache, u64::MAX).unwrap());
    assert_eq!(original.len(), 3, "two grids and an entry of bytes");
    let in_archive = entries_of(&brodo::archive::read(&archive, u64::MAX).unwrap());
    assert_eq!(in_archive.len(), 2, "two grids");
    for (input, held) in [(&cache, &original), (&archive, &in_archive)] {
        let as_cache = converted(input, &["--as", "cache"], "as.cache");
        let as_archive = converted(input, &["--as", "archive"], "as.archive");
        let head = std::fs::read(&as_archive).unwrap();
        assert_eq!(FileKind::of(&head), Some(FileKind::Archive), "{input:?}");
        let from_archive = brodo::archive::read(&as_archive, u64::MAX).unwrap();
        assert_eq!(&entries_of(&brodo::cache::read(&as_cache, u64::MAX).unwrap()), held);
        assert_eq!(&entries_of(&from_archive), held);
    }
    // One grid by name; and --as names the format whatever the output's name.
    let one = brodo::cache::read(
        &converted(&cache, &["--as", "cache", "--grid", "velocity"], "v.vdb"),
        u64::MAX,
    )
    .unwrap();
    assert_eq!(entries_of(&one), [original[1].clone()]);
    let head = std::fs::read(converted(&cache, &["--as", "archive"], "a.vtu")).unwrap();
    assert_eq!(FileKind::of(&head), Some(FileKind::Archive));
}

/// A `.vdb`'s grids are written to a cache file as read: `info` gives the same facts of
/// each, an SDF stating the convention and another level set keeping OpenVDB's `class`.
#[test]
fn convert_writes_a_vdbs_grids_to_a_cache_file() {
    let vdb = brodo_file("vdb_by14_v225.bin");
    let path = converted(&vdb, &["--as", "cache"], "vdb.cache");
    let mut out = Vec::new();
    mestolo::run(&Command::Info(Input::new(path)), &mut out).unwrap();
    let want: String = INFO_VDB
        .replacen("kind: OpenVDB file", "kind: cache file", 1)
        .lines()
        .filter(|l| !l.starts_with("skipped: "))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_eq!(String::from_utf8(out).unwrap(), want);
    assert!(want.contains(&format!("{SDF_KEY:?} = {SDF_VERSION:?}")));
}

/// A topology of one leaf at the origin.
fn one_leaf() -> Topology {
    let empty = Topology::new();
    let mut e = empty.edit();
    e.activate([0, 0, 0]).unwrap();
    e.commit().unwrap().0
}

/// A field of `t`'s one leaf, its values `value(i)` by place in the leaf.
fn filled<T: brodo::field::Stored + Copy>(
    t: &Topology,
    background: T,
    value: impl Fn(usize) -> T,
) -> Field<T> {
    let n = t.leaf_numbers().next().unwrap();
    let mut f = Field::new(t, background);
    for (i, v) in f.leaf_mut(t, n).unwrap().iter_mut().enumerate() {
        *v = value(i);
    }
    f
}

/// Runs `words` on `input`, writing at `out`, and gives the file written.
fn run_on(input: &Path, words: &[&str], out: &str) -> Vec<u8> {
    let out = fresh(out);
    let mut all = args(&words[..1]);
    all.push(input.as_os_str().to_owned());
    all.extend(args(&words[1..]));
    all.extend([OsString::from("-o"), out.clone().into_os_string()]);
    mestolo::run(&parse(all).unwrap(), &mut Vec::new()).unwrap();
    std::fs::read(out).unwrap()
}

/// A grid of each value type is described, written as `.vtu` as brodo's writer writes it,
/// and, of a scalar type, drawn as the same values of `f32` are.
#[test]
fn a_grid_of_every_value_type_is_described_converted_and_drawn() {
    let meta = |name: &str| GridMeta::new(name, [1.0; 3], "m").unwrap();
    let value = |i: usize| u8::try_from(i % 8 + i / 8 % 8 + i / 64).unwrap();
    let t = one_leaf();
    let f32s = filled(&t, 0.0_f32, |i| f32::from(value(i)));
    let f64s = filled(&t, 0.0_f64, |i| f64::from(value(i)));
    let u8s = filled(&t, 0_u8, value);
    let u32s = filled(&t, 0_u32, |i| u32::from(value(i)));
    let f32x3 = filled(&t, [0.0_f32; 3], |i| [f32::from(value(i)), 1.0, 2.0]);
    let f64x3 = filled(&t, [0.0_f64; 3], |i| [f64::from(value(i)), 1.0, 2.0]);
    let path = fresh("types.cache");
    let entries = [
        CacheEntry::new(&meta("f32"), &t, &f32s).unwrap(),
        CacheEntry::new(&meta("f64"), &t, &f64s).unwrap(),
        CacheEntry::new(&meta("u8"), &t, &u8s).unwrap(),
        CacheEntry::new(&meta("u32"), &t, &u32s).unwrap(),
        CacheEntry::new(&meta("f32x3"), &t, &f32x3).unwrap(),
        CacheEntry::new(&meta("f64x3"), &t, &f64x3).unwrap(),
    ];
    brodo::cache::write(&path, &entries).unwrap();

    let mut out = Vec::new();
    mestolo::run(&Command::Info(Input::new(path.clone())), &mut out).unwrap();
    let text = String::from_utf8(out).unwrap();
    for (i, t) in ["f32", "f64", "u8", "u32", "[f32; 3]", "[f64; 3]"].iter().enumerate() {
        assert!(text.contains(&format!("\n  type: {t}\n")), "grid {i}: {text}");
    }

    let want = |name: &str, write: &dyn Fn(&Path)| {
        let expected = fresh(&format!("{name}.expected.vtu"));
        write(&expected);
        let got = run_on(&path, &["convert", "--grid", name], &format!("{name}.vtu"));
        assert!(got == std::fs::read(&expected).unwrap(), "{name}");
    };
    want("f32", &|p| brodo::vtk::write(p, &t, &f32s, "f32").unwrap());
    want("f64", &|p| brodo::vtk::write(p, &t, &f64s, "f64").unwrap());
    want("u8", &|p| brodo::vtk::write(p, &t, &u8s, "u8").unwrap());
    want("u32", &|p| brodo::vtk::write(p, &t, &u32s, "u32").unwrap());
    want("f32x3", &|p| brodo::vtk::write(p, &t, &f32x3, "f32x3").unwrap());
    want("f64x3", &|p| brodo::vtk::write(p, &t, &f64x3, "f64x3").unwrap());

    let draw = |name: &str| {
        run_on(
            &path,
            &["slice", "--grid", name, "--axis", "z", "--at", "3"],
            &format!("{name}.png"),
        )
    };
    let of_f32 = draw("f32");
    for name in ["f64", "u8", "u32"] {
        assert!(draw(name) == of_f32, "{name}");
    }
}

/// A grid whose `class` is not "level set" is drawn as one with none, and one whose `class`
/// is, as a level set (docs/formats.md §9).
#[test]
fn a_grid_is_drawn_as_a_level_set_by_its_class() {
    let t = one_leaf();
    let f = filled(&t, 4.0_f32, |i| f32::from(u8::try_from(i % 8 + i / 8 % 8).unwrap()) - 7.0);
    let drawn = |name: &str, class: Option<&str>| {
        let mut m = GridMeta::new("g", [1.0; 3], "m").unwrap();
        if let Some(c) = class {
            m.extra.insert("class".to_owned(), c.to_owned());
        }
        let path = cache_file(&format!("{name}.cache"), &m, &t, &f);
        run_on(&path, &["slice", "--axis", "z", "--at", "3"], &format!("{name}.png"))
    };
    let plain = drawn("plain", None);
    assert!(drawn("fog", Some("fog volume")) == plain);
    assert!(drawn("level", Some("level set")) != plain);
}
