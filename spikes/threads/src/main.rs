//! Spike for B-014: the leaf-parallel update (`brodo::parallel::par_update`) on rayon,
//! against two versions on `std::thread::scope` only: a static split of the leaves into one
//! contiguous run per thread, and a dynamic one handing out blocks of `BLOCK` leaves to
//! whichever thread is free.
//!
//! The work: one explicit diffusion step per leaf, the textbook 7-point Laplacian on the
//! gather of the leaf and one ghost layer, over a narrow-band sphere SDF of radius 80.
//! "skewed" repeats the step 8 times on leaves with x ≥ 40, so that a static split leaves
//! some threads idle. Outputs are written into one `Vec<[f32; 512]>` by leaf number, as
//! `Field` holds them.
//!
//! `bench <rounds> <pairs>` measures to TESTING.md T5 (fresh process pairs, items in turn,
//! a 256 MiB eviction before each run, fastest time kept); `check` compares the outputs bit
//! for bit; `miri` runs each variant once on a small grid.

use std::hint::black_box;
use std::process::Command;
use std::sync::Mutex;
use std::time::Instant;

use brodo::field::Field;
use brodo::gather::{GATHER_VOXELS, gather};
use brodo::topology::{LeafNumber, Topology};
use brodo::tree::{Coord, TileLevel};

const BAND: f32 = 3.0;
const BLOCK: usize = 16;
type Leaf = [f32; 512];

fn sphere(r: i32) -> (Topology, Field<f32>) {
    let d = |v: Coord| v.map(|c| (c as f32).powi(2)).iter().sum::<f32>().sqrt() - r as f32;
    let lim = r + 16;
    let empty = Topology::new();
    let mut e = empty.edit();
    for z in (-lim..lim).step_by(8) {
        for y in (-lim..lim).step_by(8) {
            for x in (-lim..lim).step_by(8) {
                let ds: Vec<f32> = (0..512).map(|i| d([x + i % 8, y + i / 8 % 8, z + i / 64])).collect();
                if ds.iter().all(|v| *v <= -BAND) {
                    e.add_tile(TileLevel::Lower, [x, y, z]).unwrap();
                } else if !ds.iter().all(|v| *v >= BAND) {
                    e.activate([x, y, z]).unwrap();
                }
            }
        }
    }
    let (t, _) = e.commit().unwrap();
    let mut f = Field::new(&t, BAND);
    for n in t.leaf_numbers().collect::<Vec<_>>() {
        let vs: Vec<Coord> = t.leaf_voxels(n).unwrap().collect();
        for (v, x) in vs.iter().zip(f.leaf_mut(&t, n).unwrap().iter_mut()) {
            *x = d(*v).clamp(-BAND, BAND);
        }
    }
    for n in t.tile_numbers().collect::<Vec<_>>() {
        f.set_tile(&t, n, -BAND).unwrap();
    }
    (t, f)
}

/// One leaf's step; `skewed` repeats it 8 times on leaves with x ≥ 40.
fn step(t: &Topology, f: &Field<f32>, n: LeafNumber, out: &mut Leaf, skewed: bool) {
    let reps = if skewed && t.leaf_origin(n).unwrap()[0] >= 40 { 8 } else { 1 };
    let mut g = [0.0_f32; GATHER_VOXELS];
    gather(t, f, n, &mut g).unwrap();
    for _ in 0..reps {
        for (i, u) in out.iter_mut().enumerate() {
            let c = (i % 8 + 1) + 10 * (i / 8 % 8 + 1) + 100 * (i / 64 + 1);
            let sum = g[c - 1] + g[c + 1] + g[c - 10] + g[c + 10] + g[c - 100] + g[c + 100];
            *u = g[c] + 0.125 * (sum - 6.0 * g[c]);
        }
        black_box(&*out);
    }
}

fn numbers(t: &Topology) -> Vec<LeafNumber> {
    t.leaf_numbers().collect()
}

fn with_rayon(t: &Topology, f: &Field<f32>, out: &mut [Leaf], skewed: bool) {
    use rayon::prelude::*;
    let ns = numbers(t);
    out.par_iter_mut().zip(ns.par_iter()).for_each(|(o, n)| step(t, f, *n, o, skewed));
}

/// `THREADS` if set (Miri reports one core), else the machine's parallelism.
fn threads() -> usize {
    if let Ok(n) = std::env::var("THREADS") {
        return n.parse().unwrap();
    }
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

fn with_static(t: &Topology, f: &Field<f32>, out: &mut [Leaf], skewed: bool) {
    let ns = numbers(t);
    let per = out.len().div_ceil(threads()).max(1);
    std::thread::scope(|s| {
        for (o, n) in out.chunks_mut(per).zip(ns.chunks(per)) {
            s.spawn(move || {
                for (o, n) in o.iter_mut().zip(n) {
                    step(t, f, *n, o, skewed);
                }
            });
        }
    });
}

fn with_dynamic(t: &Topology, f: &Field<f32>, out: &mut [Leaf], skewed: bool) {
    let ns = numbers(t);
    let blocks = Mutex::new(out.chunks_mut(BLOCK).zip(ns.chunks(BLOCK)));
    std::thread::scope(|s| {
        for _ in 0..threads() {
            s.spawn(|| loop {
                let Some((o, n)) = blocks.lock().unwrap().next() else { break };
                for (o, n) in o.iter_mut().zip(n) {
                    step(t, f, *n, o, skewed);
                }
            });
        }
    });
}

type Variant = (&'static str, fn(&Topology, &Field<f32>, &mut [Leaf], bool), bool);

fn variants() -> Vec<Variant> {
    let mut v: Vec<Variant> = Vec::new();
    for skewed in [false, true] {
        v.push(("rayon", with_rayon, skewed));
        v.push(("static", with_static, skewed));
        v.push(("dynamic", with_dynamic, skewed));
    }
    v
}

fn name(v: &Variant) -> String {
    format!("{}_{}", v.0, if v.2 { "skewed" } else { "uniform" })
}

fn child(rounds: usize) {
    let r = std::env::var("RADIUS").map_or(80, |v| v.parse().unwrap());
    let (t, f) = sphere(r);
    let mut out = vec![[0.0_f32; 512]; t.leaf_count()];
    let mut evict = vec![0_u8; 256 << 20];
    let vs = variants();
    let mut best = vec![u128::MAX; vs.len()];
    for r in 0..rounds {
        for k in 0..vs.len() {
            let j = (r + k) % vs.len();
            for (i, b) in evict.iter_mut().step_by(64).enumerate() {
                *b = b.wrapping_add(i as u8);
            }
            black_box(&evict);
            let s = Instant::now();
            (vs[j].1)(&t, &f, &mut out, vs[j].2);
            best[j] = best[j].min(s.elapsed().as_nanos());
        }
    }
    for (v, b) in vs.iter().zip(best) {
        println!("{} {b}", name(v));
    }
}

fn load() -> f64 {
    let out = Command::new("sysctl").args(["-n", "vm.loadavg"]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).split_whitespace().nth(1).unwrap().parse().unwrap()
}

fn bench(rounds: usize, pairs: usize) {
    println!("load {:.2}, {} threads", load(), threads());
    let run = || {
        let out = Command::new(std::env::current_exe().unwrap()).args(["child", &rounds.to_string()]).output().unwrap();
        String::from_utf8(out.stdout).unwrap().lines().map(|l| {
            let (n, ns) = l.split_once(' ').unwrap();
            (n.to_owned(), ns.parse::<f64>().unwrap())
        }).collect::<Vec<_>>()
    };
    for p in 0..pairs {
        let (a, b) = (run(), run());
        for ((n, x), (_, y)) in a.iter().zip(&b) {
            println!("pair {p} {n:<16} {:.3} / {:.3} ms ({:.1} % apart)", x / 1e6, y / 1e6, (x - y).abs() / x.min(*y) * 100.0);
        }
    }
    println!("load {:.2}", load());
}

/// Every variant gives the same bits as rayon, on both workloads.
fn check() {
    let r = std::env::var("RADIUS").map_or(80, |v| v.parse().unwrap());
    let (t, f) = sphere(r);
    for skewed in [false, true] {
        let mut want = vec![[0.0_f32; 512]; t.leaf_count()];
        with_rayon(&t, &f, &mut want, skewed);
        for (label, run) in [("static", with_static as fn(&Topology, &Field<f32>, &mut [Leaf], bool)), ("dynamic", with_dynamic)] {
            let mut got = vec![[0.0_f32; 512]; t.leaf_count()];
            run(&t, &f, &mut got, skewed);
            let same = want.iter().flatten().zip(got.iter().flatten()).all(|(a, b)| a.to_bits() == b.to_bits());
            println!("{label} skewed={skewed}: {} leaves, same bits as rayon: {same}", t.leaf_count());
        }
    }
}

/// One run of each variant on a small grid, for Miri.
fn miri(which: &str) {
    let (t, f) = sphere(4);
    let mut out = vec![[0.0_f32; 512]; t.leaf_count()];
    match which {
        "rayon" => {
            with_rayon(&t, &f, &mut out, false);
            println!("rayon pool: {} threads", rayon::current_num_threads());
        }
        "static" => with_static(&t, &f, &mut out, false),
        _ => with_dynamic(&t, &f, &mut out, false),
    }
    println!("{which}: {} leaves done on {} threads", t.leaf_count(), threads());
}

/// Ten updates side by side under an outer rayon `par_iter`, as a caller already using rayon
/// would run them, each with the inner variant named; fastest of `rounds`, in turn.
fn nested(rounds: usize) {
    use rayon::prelude::*;
    let r = std::env::var("RADIUS").map_or(80, |v| v.parse().unwrap());
    let (t, f) = sphere(r);
    let mut outs = vec![vec![[0.0_f32; 512]; t.leaf_count()]; 10];
    let inner: [(&str, fn(&Topology, &Field<f32>, &mut [Leaf], bool)); 4] = [
        ("rayon", with_rayon),
        ("dynamic", with_dynamic),
        ("dynamic_1_thread", |t, f, o, s| {
            let ns = numbers(t);
            for (o, n) in o.iter_mut().zip(&ns) {
                step(t, f, *n, o, s);
            }
        }),
        ("sequential_outer", with_rayon),
    ];
    let mut best = [u128::MAX; 4];
    for round in 0..rounds {
        for k in 0..4 {
            let j = (round + k) % 4;
            let s = Instant::now();
            if j == 3 {
                for o in outs.iter_mut() {
                    with_rayon(&t, &f, o, false);
                }
            } else {
                outs.par_iter_mut().for_each(|o| (inner[j].1)(&t, &f, o, false));
            }
            best[j] = best[j].min(s.elapsed().as_nanos());
        }
    }
    println!("{} leaves, 10 updates", t.leaf_count());
    for ((n, _), b) in inner.iter().zip(best) {
        println!("{n:<18} {:.3} ms", b as f64 / 1e6);
    }
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    match a.first().map(String::as_str) {
        Some("child") => child(a[1].parse().unwrap()),
        Some("bench") => bench(a[1].parse().unwrap(), a[2].parse().unwrap()),
        Some("miri") => miri(&a[1]),
        Some("nested") => nested(a[1].parse().unwrap()),
        _ => check(),
    }
}
