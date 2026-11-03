//! Format tier (TESTING.md §1) for mestolo (ADR-0050): each command's output from brodo's
//! committed files held byte for byte to a committed golden file, and the program run for
//! its command line, its exit status and its refusal to replace a file.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command as Process;

use brodo::cache::CacheEntry;
use brodo::field::Field;
use brodo::meta::GridMeta;
use brodo::topology::Topology;
use brodo::tree::TileLevel;
use mestolo::args::{Command, DEFAULT_LIMIT, Input, Output, parse};
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
    assert_eq!(info("cache_v1.bin"), INFO_CACHE);
    let archive = INFO_CACHE.replacen("kind: cache file", "kind: archive", 1);
    assert_eq!(info("archive_v1.bin"), archive);
    assert_eq!(info("vdb_by14_v225.bin"), INFO_VDB);
    // Version 2: the same grids, an entry of bytes listed by its name and length.
    let v2 = INFO_CACHE.replacen("grids: 2\n", "grids: 2\nbytes: \"readme\", 11 bytes\n", 1);
    assert_eq!(info("cache_v2.bin"), v2);
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
    let cache = brodo_file("cache_v1.bin");
    let path = cache.to_str().unwrap();
    assert_eq!(written(&["convert", path, "--grid", "density"], &fresh("c.vdb")), VDB);
    assert_eq!(written(&["convert", path, "--grid", "density"], &fresh("c.vtu")), VTU);
}

#[test]
fn what_a_format_does_not_hold_is_refused_by_name() {
    let cache = brodo_file("cache_v1.bin");
    let run = |words: &[&str], out: &str| {
        let mut all = args(words);
        all.extend([OsString::from("-o"), fresh(out).into_os_string()]);
        mestolo::run(&parse(all).unwrap(), &mut Vec::new())
    };
    let p = cache.to_str().unwrap();
    assert!(matches!(
        run(&["convert", p], "all.vdb"),
        Err(MestoloError::NotWritten { value_type: "[f32; 3]", .. })
    ));
    assert!(matches!(run(&["convert", p], "all.vtu"), Err(MestoloError::WhichGrid(2))));
    assert!(matches!(
        run(&["convert", p, "--grid", "density"], "c.png"),
        Err(MestoloError::OutputKind(_))
    ));
    assert!(matches!(
        run(&["slice", p, "--grid", "velocity", "--axis", "x", "--at", "0"], "v.png"),
        Err(MestoloError::NotScalar("[f32; 3]"))
    ));
    assert!(matches!(
        run(&["slice", p, "--grid", "none", "--axis", "x", "--at", "0"], "n.png"),
        Err(MestoloError::NoGrid(n)) if n == "none"
    ));
}

#[test]
fn a_file_of_no_kind_read_and_a_bare_grid_are_refused() {
    let run =
        |name: &str| mestolo::run(&Command::Info(Input::new(brodo_file(name))), &mut Vec::new());
    assert!(matches!(run("frozen_v1_f32.bin"), Err(MestoloError::BareGrid(_))));
    assert!(matches!(run("slice_sphere.png"), Err(MestoloError::UnknownKind(_))));
}

#[test]
fn the_limit_bounds_each_kind_of_file() {
    for name in ["cache_v1.bin", "archive_v1.bin", "vdb_by14_v225.bin"] {
        let words = ["info", "--limit", "1024"];
        let mut all = args(&words);
        all.push(brodo_file(name).into_os_string());
        let refused = mestolo::run(&parse(all).unwrap(), &mut Vec::new());
        assert!(matches!(refused, Err(MestoloError::Cache(_) | MestoloError::Vdb(_))), "{name}");
    }
}

#[test]
fn the_command_line_is_parsed_and_what_it_does_not_take_named() {
    let slice = parse(args(&["slice", "f", "--axis", "y", "--at", "-3", "-o", "s.png", "--force"]));
    let Ok(Command::Slice { input, grid: None, at: -3, output, .. }) = slice else {
        panic!("{slice:?}")
    };
    assert_eq!((input.path, input.limit), (PathBuf::from("f"), DEFAULT_LIMIT));
    assert_eq!((output.path, output.force), (PathBuf::from("s.png"), true));
    assert_eq!(parse(args(&[])).unwrap(), Command::Help);
    let refused = |words: &[&str]| parse(args(words)).unwrap_err();
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
    let cache = brodo_file("cache_v1.bin");
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
    let help = Process::new(exe).output().unwrap();
    assert_eq!((help.status.code(), help.stdout.as_slice()), (Some(0), mestolo::USAGE.as_bytes()));
}

#[test]
fn an_output_is_refused_before_its_input_is_read() {
    let out = fresh("there.png");
    std::fs::write(&out, b"kept").unwrap();
    let output = Output::new(out.clone(), false);
    let command =
        Command::Convert { input: Input::new(PathBuf::from("/no/such/file")), grid: None, output };
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
    let loaded = mestolo::load::load(&Input::new(brodo_file("cache_v1.bin"))).unwrap();
    for name in ["density", "velocity"] {
        assert_eq!(loaded.grid(Some(name)).unwrap().meta.name, name);
    }
}
