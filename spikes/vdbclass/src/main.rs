//! Reads a `.vdb` with brodo's reader and prints, for each grid, what the conditions of
//! D5's option B are measured on: its value type and `class`, the spread of its voxel size
//! in units of `f64::EPSILON`, its background, its leaves' least and greatest values and
//! how many lie outside ±background and by how many `f32` steps at most, and its tiles'
//! values against ±background.
//!
//!   vdbclass-spike FILE
//!   vdbclass-spike brodo N   N grids brodo writes and reads back, under rotated maps of
//!                            five voxel sizes, as the same listing's spread lines

use brodo::field::{AnyField, Field};
use brodo::meta::{GridMeta, SDF_KEY, SDF_VERSION};
use brodo::topology::Topology;
use brodo::vdb::{VdbEntry, VdbValue, from_bytes, read, to_bytes};

/// `n` grids of one voxel under maps of voxel size h, rotated by pseudo-random unit
/// quaternions and translated, written by brodo and read back, each listed.
fn brodo_rotations(n: usize) {
    let empty = Topology::new();
    let mut e = empty.edit();
    e.activate([0, 0, 0]).unwrap();
    let t = e.commit().unwrap().0;
    let f = Field::new(&t, 0.3_f32);
    let mut s: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut rnd = || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        (s >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    };
    for i in 0..n {
        let h = [1.0, 0.1, 0.05, 1.0 / 3.0, 0.0123][i % 5];
        let (mut w, mut x, mut y, mut z) = (rnd(), rnd(), rnd(), rnd());
        let l = (w * w + x * x + y * y + z * z).sqrt();
        (w, x, y, z) = (w / l, x / l, y / l, z / l);
        let r = [
            [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w)],
            [2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w)],
            [2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y)],
        ];
        let mut m = GridMeta::new(&format!("b{i:05}"), [h; 3], "m").unwrap();
        for (row, r) in m.transform.iter_mut().zip(r) {
            for (a, b) in row.iter_mut().zip(r) {
                *a = h * b;
            }
            row[3] = rnd() * 10.0;
        }
        m.extra.insert(SDF_KEY.to_owned(), SDF_VERSION.to_owned());
        let bytes = to_bytes(&[VdbEntry::new(&m, &t, &f).unwrap()]).unwrap();
        let g = &from_bytes(&bytes, "m", u64::MAX).unwrap().grids[0];
        let AnyField::F32(f) = &g.field else { unreachable!() };
        report(&g.meta, &g.topology, f, "f32");
    }
}

/// The `f32` steps between the magnitudes of `a` and `b`.
fn steps(a: f32, b: f32) -> u32 {
    a.abs().to_bits().abs_diff(b.abs().to_bits())
}

fn report<T: VdbValue + Into<f64>>(meta: &GridMeta, t: &Topology, f: &Field<T>, kind: &str) {
    let bg: f64 = f.background().into();
    let class = meta.extra.get("class").map_or("-", String::as_str);
    let v = meta.voxel_size;
    let lo = v.iter().copied().fold(f64::MAX, f64::min);
    let hi = v.iter().copied().fold(0.0, f64::max);
    let spread = (hi - lo) / lo / f64::EPSILON;
    let (mut min, mut max) = (f64::MAX, f64::MIN);
    let (mut over, mut worst, mut nan, mut leaves) = (0u64, 0u32, 0u64, 0u64);
    for n in t.leaf_numbers() {
        leaves += 1;
        for x in f.leaf(t, n).unwrap() {
            let x: f64 = (*x).into();
            if x.is_nan() {
                nan += 1;
                continue;
            }
            min = min.min(x);
            max = max.max(x);
            if x.abs() > bg {
                over += 1;
                worst = worst.max(steps(x as f32, bg as f32));
            }
        }
    }
    let (mut tiles, mut odd) = (0u64, 0u64);
    for n in t.tile_numbers() {
        tiles += 1;
        let x: f64 = f.tile(t, n).unwrap().into();
        if x != bg && x != -bg {
            odd += 1;
        }
    }
    let ok = kind == "f32" && bg > 0.0 && bg.is_finite() && over == 0 && nan == 0 && odd == 0;
    println!(
        "{}\t{kind}\t{class}\tvoxel={v:?}\tspread={spread:.2}eps\tbg={bg}\tleaves={leaves}\t\
         leaf=[{min},{max}]\tover={over}\tworst={worst}steps\tnan={nan}\ttiles={tiles}\t\
         odd_tiles={odd}\tB16={}\tBexact={}",
        meta.name,
        ok && spread <= 16.0,
        ok && spread == 0.0,
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("brodo") {
        brodo_rotations(args[2].parse().unwrap());
        return;
    }
    let path = args.get(1).expect("usage: vdbclass-spike FILE | vdbclass-spike brodo N");
    let file = read(std::path::Path::new(&path), "m", u64::MAX).unwrap();
    for g in &file.grids {
        match &g.field {
            AnyField::F32(f) => report(&g.meta, &g.topology, f, "f32"),
            AnyField::F64(f) => report(&g.meta, &g.topology, f, "f64"),
            _ => unreachable!(),
        }
    }
    println!("skipped {:?}", file.skipped);
}
