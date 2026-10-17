//! Spike for B-007: a benchmark harness of brodo's own, std only, built to TESTING.md T5:
//! the fastest of many runs of each variant, the variants run in turn, each run after the
//! caches are evicted, each comparison in a fresh process, the whole run twice.
//!
//! The parent (`bench <evict MiB> <rounds> <pairs>`) runs `pairs` pairs of child processes;
//! each child builds the workload, then for `rounds` rounds runs every variant once, in
//! turn, each after writing over an `evict`-MiB buffer, and prints each variant's fastest
//! and median time. The parent reports, per variant, how far the two children of each pair
//! agree on the fastest time.
//!
//! The workload: a narrow-band sphere SDF of radius 80 (the textbook |p| − r), read at
//! 100 000 voxels by three variants: random voxels through a `ValueAccessor`, the same
//! voxels through `Field::value_at`, and a sweep of a box x fastest through an accessor.

use std::hint::black_box;
use std::process::Command;
use std::time::Instant;

use brodo::accessor::ValueAccessor;
use brodo::field::Field;
use brodo::topology::Topology;
use brodo::tree::{Coord, TileLevel};

const BAND: f32 = 3.0;
const R: i32 = 80;
const LOOKUPS: usize = 100_000;

fn sphere() -> (Topology, Field<f32>) {
    let d = |v: Coord| (v.map(|c| c as f32).iter().map(|c| c * c).sum::<f32>()).sqrt() - R as f32;
    let lim = R + 16;
    let empty = Topology::new();
    let mut e = empty.edit();
    for z in (-lim..lim).step_by(8) {
        for y in (-lim..lim).step_by(8) {
            for x in (-lim..lim).step_by(8) {
                let vs: Vec<f32> = (0..512).map(|i| d([x + i % 8, y + i / 8 % 8, z + i / 64])).collect();
                if vs.iter().all(|v| *v <= -BAND) {
                    e.add_tile(TileLevel::Lower, [x, y, z]).unwrap();
                } else if !vs.iter().all(|v| *v >= BAND) {
                    e.activate([x, y, z]).unwrap();
                }
            }
        }
    }
    let (t, _) = e.commit().unwrap();
    let mut f = Field::new(&t, BAND);
    for n in t.leaf_numbers().collect::<Vec<_>>() {
        let o = t.leaf_origin(n).unwrap();
        for (i, v) in (0..).zip(f.leaf_mut(&t, n).unwrap().iter_mut()) {
            *v = d([o[0] + i % 8, o[1] + i / 8 % 8, o[2] + i / 64]).clamp(-BAND, BAND);
        }
    }
    for n in t.tile_numbers().collect::<Vec<_>>() {
        f.set_tile(&t, n, -BAND).unwrap();
    }
    (t, f)
}

fn random_voxels() -> Vec<Coord> {
    let mut g = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = || {
        g = g.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((g >> 33) % (2 * (R as u64 + 16))) as i32 - (R + 16)
    };
    (0..LOOKUPS).map(|_| [next(), next(), next()]).collect()
}

/// Writes over the whole buffer, a cache line at a time, so what the last run left in the
/// caches is gone.
fn evict(buffer: &mut [u8]) {
    for (i, b) in buffer.iter_mut().step_by(64).enumerate() {
        *b = b.wrapping_add(i as u8);
    }
    black_box(&*buffer);
}

fn child(evict_mib: usize, rounds: usize) {
    let (t, f) = sphere();
    let voxels = random_voxels();
    let side = 46; // 46³ ≈ 97 000 voxels, swept x fastest
    let sweep: Vec<Coord> = (0..side * side * side)
        .map(|i| [i % side, i / side % side, i / (side * side)].map(|c| c as i32 + R - 30))
        .collect();
    let mut buffer = vec![0_u8; evict_mib << 20];
    let variants: [(&str, &dyn Fn() -> f32); 3] = [
        ("accessor_random", &|| {
            let mut acc = ValueAccessor::new(&t, &f).unwrap();
            voxels.iter().map(|v| acc.get(*v)).sum()
        }),
        ("value_at_random", &|| voxels.iter().map(|v| f.value_at(&t, *v).unwrap()).sum()),
        ("accessor_sweep", &|| {
            let mut acc = ValueAccessor::new(&t, &f).unwrap();
            sweep.iter().map(|v| acc.get(*v)).sum()
        }),
    ];
    let mut times = vec![Vec::with_capacity(rounds); variants.len()];
    for round in 0..rounds {
        // In turn, starting from a different variant each round.
        for k in 0..variants.len() {
            let j = (round + k) % variants.len();
            evict(&mut buffer);
            let start = Instant::now();
            black_box(variants[j].1());
            times[j].push(start.elapsed().as_nanos());
        }
    }
    for ((name, _), ts) in variants.iter().zip(&mut times) {
        ts.sort_unstable();
        println!("{name} {} {}", ts[0], ts[ts.len() / 2]);
    }
}

fn run_child(evict_mib: usize, rounds: usize) -> Vec<(String, f64, f64)> {
    let out = Command::new(std::env::current_exe().unwrap())
        .args(["child", &evict_mib.to_string(), &rounds.to_string()])
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| {
            let w: Vec<&str> = l.split(' ').collect();
            (w[0].to_owned(), w[1].parse().unwrap(), w[2].parse().unwrap())
        })
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let num = |i: usize| args[i].parse::<usize>().unwrap();
    if args.get(1).map(String::as_str) == Some("child") {
        return child(num(2), num(3));
    }
    let (evict_mib, rounds, pairs) = (num(1), num(2), num(3));
    let start = Instant::now();
    for pair in 0..pairs {
        let a = run_child(evict_mib, rounds);
        let b = run_child(evict_mib, rounds);
        for ((name, fa, ma), (_, fb, mb)) in a.iter().zip(&b) {
            let spread = (fa - fb).abs() / fa.min(*fb) * 100.0;
            println!(
                "evict {evict_mib} MiB, {rounds} rounds, pair {pair}: {name}: fastest {:.3} / {:.3} ms ({spread:.1} % apart), median {:.3} / {:.3} ms",
                fa / 1e6, fb / 1e6, ma / 1e6, mb / 1e6
            );
        }
    }
    println!("took {:.1} s", start.elapsed().as_secs_f64());
}
