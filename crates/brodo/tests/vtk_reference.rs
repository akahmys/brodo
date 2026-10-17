//! Integration tier (TESTING.md §1): a field written as `.vtu` reads back, by a minimal
//! parser written here from VTK's file-format documentation, as one cell per leaf voxel and
//! one per tile, each at its place with its size, its eight corners in VTK_VOXEL's order
//! and the value written to it; points are shared, never repeated; the same input gives the
//! same bytes.

#![cfg(feature = "vtk")]

use std::collections::{BTreeMap, BTreeSet};

use brodo::field::Field;
use brodo::topology::Topology;
use brodo::tree::{Coord, TileLevel};
use brodo::vtk::write_vtu;
use proptest::prelude::*;

/// A file read back: the piece's counts and its arrays' tokens, by `Name` (points as
/// "Points").
struct Parsed {
    points: usize,
    cells: usize,
    arrays: BTreeMap<String, Vec<String>>,
}

fn attribute<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let start = line.find(&format!("{key}=\""))? + key.len() + 2;
    line[start..].split('"').next()
}

fn parse(text: &str) -> Parsed {
    let mut parsed = Parsed { points: 0, cells: 0, arrays: BTreeMap::new() };
    let mut current: Option<String> = None;
    for line in text.lines() {
        if line.starts_with("<Piece ") {
            parsed.points = attribute(line, "NumberOfPoints").unwrap().parse().unwrap();
            parsed.cells = attribute(line, "NumberOfCells").unwrap().parse().unwrap();
        } else if line.starts_with("<DataArray ") {
            let name = attribute(line, "Name").unwrap_or("Points").to_owned();
            assert!(line.contains(r#"format="ascii""#), "{line}");
            parsed.arrays.insert(name.clone(), Vec::new());
            current = Some(name);
        } else if line == "</DataArray>" {
            current = None;
        } else if let Some(name) = &current {
            parsed.arrays.get_mut(name).unwrap().extend(line.split_whitespace().map(str::to_owned));
        }
    }
    parsed
}

/// The value the test writes at a leaf voxel or a tile's origin; now and then a NaN.
fn value(v: Coord) -> f32 {
    let h = v.iter().fold(17_i64, |h, c| h.wrapping_mul(31).wrapping_add(i64::from(*c)));
    if h.rem_euclid(97) == 0 {
        f32::NAN
    } else {
        f32::from(i16::try_from(h.rem_euclid(2000)).unwrap()) / 7.0 - 100.0
    }
}

fn coord(range: std::ops::Range<i32>) -> impl Strategy<Value = Coord> {
    [range.clone(), range.clone(), range]
}

fn grid(leaves: &[Coord], lower: &[Coord], upper: &[Coord]) -> (Topology, Field<f32>) {
    let empty = Topology::new();
    let mut e = empty.edit();
    for v in leaves {
        e.activate(*v).unwrap();
    }
    for (level, vs) in [(TileLevel::Lower, lower), (TileLevel::Upper, upper)] {
        for v in vs {
            // A region already holding a leaf or tile is refused; the test keeps what was added.
            let _occupied = e.add_tile(level, *v);
        }
    }
    let (t, _) = e.commit().unwrap();
    let mut f = Field::new(&t, 0.5_f32);
    for n in t.leaf_numbers().collect::<Vec<_>>() {
        let o = t.leaf_origin(n).unwrap();
        for (i, x) in (0..).zip(f.leaf_mut(&t, n).unwrap().iter_mut()) {
            *x = value([o[0] + i % 8, o[1] + i / 8 % 8, o[2] + i / 64]);
        }
    }
    for n in t.tile_numbers().collect::<Vec<_>>() {
        let (_, o) = t.tile(n).unwrap();
        f.set_tile(&t, n, value(o)).unwrap();
    }
    (t, f)
}

fn same(a: f32, b: f32) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn cells_read_back_as_the_leaves_and_tiles_written(
        leaves in prop::collection::vec(coord(-40..40), 0..12),
        lower in prop::collection::vec(coord(-40..40), 0..6),
        upper in prop::collection::vec(coord(-300..300), 0..2),
    ) {
        let (t, f) = grid(&leaves, &lower, &upper);
        let mut bytes = Vec::new();
        write_vtu(&t, &f, "value", &mut bytes).unwrap();
        let mut again = Vec::new();
        write_vtu(&t, &f, "value", &mut again).unwrap();
        prop_assert!(bytes == again, "the same input gave different bytes");
        let p = parse(std::str::from_utf8(&bytes).unwrap());
        let cell_count = t.leaf_count() * 512 + t.tile_count();
        prop_assert_eq!(p.cells, cell_count);

        let points: Vec<[i64; 3]> = p.arrays["Points"]
            .as_chunks::<3>()
            .0
            .iter()
            // Written as integers: every corner lies on the voxel lattice.
            .map(|c| c.clone().map(|x| x.parse::<i64>().unwrap()))
            .collect();
        prop_assert_eq!(points.len(), p.points);
        prop_assert_eq!(points.iter().collect::<BTreeSet<_>>().len(), points.len(), "a point repeats");
        let conn: Vec<usize> = p.arrays["connectivity"].iter().map(|s| s.parse().unwrap()).collect();
        let offsets: Vec<usize> = p.arrays["offsets"].iter().map(|s| s.parse().unwrap()).collect();
        prop_assert_eq!(offsets, (1..=cell_count).map(|k| k * 8).collect::<Vec<_>>());
        prop_assert!(p.arrays["types"].iter().all(|s| s == "11"));
        let values: Vec<f32> = p.arrays["value"].iter().map(|s| s.parse().unwrap()).collect();
        prop_assert_eq!(values.len(), cell_count);

        let leaf_origins: BTreeSet<Coord> = t.leaf_numbers().map(|n| t.leaf_origin(n).unwrap()).collect();
        let mut tiles: BTreeSet<([i64; 3], i64)> = t
            .tile_numbers()
            .map(|n| {
                let (level, o) = t.tile(n).unwrap();
                (o.map(i64::from), 1_i64 << level.edge_log2())
            })
            .collect();
        let mut voxels = BTreeSet::new();
        for (corners, v) in conn.as_chunks::<8>().0.iter().zip(&values) {
            let c: Vec<[i64; 3]> = corners.iter().map(|&i| points[i]).collect();
            let (o, e) = (c[0], c[7][0] - c[0][0]);
            for (k, corner) in (0_i64..).zip(&c) {
                let want = [o[0] + (k & 1) * e, o[1] + (k >> 1 & 1) * e, o[2] + (k >> 2) * e];
                prop_assert_eq!(*corner, want, "corner {} of the cell at {:?}", k, o);
            }
            let origin = o.map(|x| i32::try_from(x).unwrap());
            prop_assert!(same(*v, value(origin)), "{} against {} at {:?}", v, value(origin), o);
            if e == 1 {
                prop_assert!(leaf_origins.contains(&origin.map(|x| x & !7)), "{:?} is in no leaf", o);
                prop_assert!(voxels.insert(origin), "{:?} twice", o);
            } else {
                prop_assert!(tiles.remove(&(o, e)), "no tile at {:?} of edge {}", o, e);
            }
        }
        prop_assert_eq!(voxels.len(), t.leaf_count() * 512);
        prop_assert!(tiles.is_empty(), "tiles not written: {:?}", tiles);
    }
}

/// Writes a sphere's SDF and a grid with NaNs and tiles of both levels to `out/vtk/brodo/`,
/// with samples (cell centres and values) in the form `spikes/vtk/check.py` reads, for
/// reading back in VTK:
/// `out/vtk-venv/bin/python -I spikes/vtk/check.py out/vtk/brodo`.
#[test]
#[ignore = "needs VTK, which the repository does not hold, to read out/vtk/brodo/ back"]
fn files_for_vtk() {
    let r = 40;
    let d = |v: Coord| v.map(|c| f64::from(c).powi(2)).iter().sum::<f64>().sqrt() - f64::from(r);
    let lim = r + 16;
    let empty = Topology::new();
    let mut e = empty.edit();
    let mut lower = Vec::new();
    for z in (-lim..lim).step_by(8) {
        for y in (-lim..lim).step_by(8) {
            for x in (-lim..lim).step_by(8) {
                let ds: Vec<f64> =
                    (0..512).map(|i| d([x + i % 8, y + i / 8 % 8, z + i / 64])).collect();
                if ds.iter().all(|v| *v <= -3.0) {
                    lower.push([x, y, z]);
                } else if !ds.iter().all(|v| *v >= 3.0) {
                    e.activate([x, y, z]).unwrap();
                }
            }
        }
    }
    for v in lower {
        e.add_tile(TileLevel::Lower, v).unwrap();
    }
    let (t, _) = e.commit().unwrap();
    let mut f = Field::new(&t, 3.0_f32);
    for n in t.leaf_numbers().collect::<Vec<_>>() {
        let o = t.leaf_origin(n).unwrap();
        for (i, x) in (0..).zip(f.leaf_mut(&t, n).unwrap().iter_mut()) {
            #[expect(clippy::cast_possible_truncation, reason = "a distance in a band of ±3")]
            let v = d([o[0] + i % 8, o[1] + i / 8 % 8, o[2] + i / 64]).clamp(-3.0, 3.0) as f32;
            *x = v;
        }
    }
    for n in t.tile_numbers().collect::<Vec<_>>() {
        f.set_tile(&t, n, -3.0).unwrap();
    }
    write_for_vtk("r40", &t, &f);
    let leaves: Vec<Coord> = (0..40).map(|k| [k * 8 - 160, (k % 5) * 8, (k % 3) * 8]).collect();
    let (t, f) = grid(&leaves, &[[0, 64, 0], [8, 64, 0]], &[[512, 0, 0], [-640, -256, 128]]);
    write_for_vtk("rmix", &t, &f);
}

fn write_for_vtk(tag: &str, t: &Topology, f: &Field<f32>) {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/vtk/brodo").join(tag);
    std::fs::create_dir_all(&dir).unwrap();
    write_vtu(t, f, "value", std::fs::File::create(dir.join("sphere.vtu")).unwrap()).unwrap();
    let mut samples = String::new();
    let mut k = 0;
    for n in t.leaf_numbers() {
        let o = t.leaf_origin(n).unwrap();
        for (i, v) in (0..).zip(f.leaf(t, n).unwrap()) {
            k += 1;
            if k % 97 == 0 {
                let c = [o[0] + i % 8, o[1] + i / 8 % 8, o[2] + i / 64].map(|x| f64::from(x) + 0.5);
                samples.push_str(&format!("{} {} {} {}\n", c[0], c[1], c[2], v));
            }
        }
    }
    for n in t.tile_numbers() {
        let (level, o) = t.tile(n).unwrap();
        let half = f64::from(1_u32 << level.edge_log2()) / 2.0;
        let c = o.map(|x| f64::from(x) + half);
        samples.push_str(&format!("{} {} {} {}\n", c[0], c[1], c[2], f.tile(t, n).unwrap()));
    }
    std::fs::write(dir.join("samples.txt"), samples).unwrap();
}
