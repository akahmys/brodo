//! Cost measurements (TESTING.md T5): the variants of one comparison run in turn, each run
//! after evicting the caches, and the fastest of `ROUNDS` runs is kept. One comparison per
//! process: `measure remap` or `measure update`.

use std::hint::black_box;
use std::sync::Arc;
use std::time::{Duration, Instant};

use remap_spike::core::{FieldData, LEAF_VOXELS, Op, Region, Shape, SpikeError, uniform};
use remap_spike::{brand, fieldset, generation};

const ROUNDS: usize = 30;
const LEAVES: i32 = 10_000;
const TILES: i32 = 2_000;
const EVICT_BYTES: usize = 256 << 20;

fn leaf_region(i: i32) -> Region {
    [i % 22, (i / 22) % 22, i / 484]
}

fn initial_ops() -> Vec<Op> {
    let leaves = (0..LEAVES).map(|i| Op::Activate(leaf_region(i).map(|c| c * 8)));
    let tiles = (0..TILES).map(|i| Op::AddTile([i % 40, i / 40, 30]));
    leaves.chain(tiles).collect()
}

/// 1000 splits, 1000 merges of uniform leaves, 1000 activations from the background.
fn edit_ops() -> Vec<Op> {
    let splits = (0..1000).map(|i| Op::Split([i % 40, i / 40, 30]));
    let merges = (0..1000).map(|i| Op::Merge(leaf_region(i * 10)));
    let new = (0..1000).map(|i| Op::Activate([i % 40 * 8, i / 40 * 8, 40 * 8]));
    splits.chain(merges).chain(new).collect()
}

/// Values varying within a leaf, except the leaves to be merged.
fn fill<T: remap_spike::core::Value>(shape: &Shape, f: &mut FieldData<T>, v: impl Fn(usize) -> T) {
    for (i, r) in shape.leaves.iter().enumerate() {
        let merged = (0..LEAVES).step_by(10).any(|k| leaf_region(k) == *r);
        for (k, x) in f.leaf_mut(i as u32).iter_mut().enumerate() {
            *x = v(if merged { 0 } else { k });
        }
    }
}

fn evict(buf: &mut [u8], round: usize) {
    buf.fill(round as u8);
    black_box(&buf);
}

fn fastest(times: &[Duration]) -> f64 {
    times.iter().min().map_or(f64::NAN, |d| d.as_secs_f64() * 1e6)
}

fn remap_a() -> Duration {
    let (t, _) = generation::Topology::new().commit(&initial_ops()).unwrap();
    let mut a = generation::Field::new(&t, 0.0_f32);
    let mut b = generation::Field::new(&t, [0.0_f32; 3]);
    let mut c = generation::Field::new(&t, 0_u8);
    fill(t.shape(), a.data_mut(&t).unwrap(), |k| k as f32);
    fill(t.shape(), b.data_mut(&t).unwrap(), |k| [k as f32; 3]);
    fill(t.shape(), c.data_mut(&t).unwrap(), |k| k as u8);
    let ops = edit_ops();
    let start = Instant::now();
    let (t2, r) = t.commit(&ops).unwrap();
    let out = (a.remap(&r, &uniform).unwrap(), b.remap(&r, &uniform).unwrap(), c.remap(&r, &uniform).unwrap());
    let d = start.elapsed();
    black_box((t2, out));
    d
}

fn remap_b() -> Duration {
    let mut set = fieldset::FieldSet::new();
    set.commit(&initial_ops()).unwrap();
    let a = set.add(0.0_f32, Arc::new(uniform::<f32>));
    let b = set.add([0.0_f32; 3], Arc::new(uniform::<[f32; 3]>));
    let c = set.add(0_u8, Arc::new(uniform::<u8>));
    let shape = set.shape().clone();
    fill(&shape, set.get_mut(a).unwrap(), |k| k as f32);
    fill(&shape, set.get_mut(b).unwrap(), |k| [k as f32; 3]);
    fill(&shape, set.get_mut(c).unwrap(), |k| k as u8);
    let ops = edit_ops();
    let start = Instant::now();
    set.commit(&ops).unwrap();
    let d = start.elapsed();
    black_box(set);
    d
}

fn remap_c() -> Duration {
    brand::with_topology(|t0| {
        t0.commit(&initial_ops(), |t, _| {
            let mut a = brand::Field::new(&t, 0.0_f32);
            let mut b = brand::Field::new(&t, [0.0_f32; 3]);
            let mut c = brand::Field::new(&t, 0_u8);
            fill(t.shape(), a.data_mut(&t), |k| k as f32);
            fill(t.shape(), b.data_mut(&t), |k| [k as f32; 3]);
            fill(t.shape(), c.data_mut(&t), |k| k as u8);
            let ops = edit_ops();
            let start = Instant::now();
            let r = t.commit(&ops, |_t2, r| -> Result<usize, SpikeError> {
                let out = (r.remap(&a, &uniform)?, r.remap(&b, &uniform)?, r.remap(&c, &uniform)?);
                Ok(black_box(out).0.into_data().leaf_values.len())
            });
            let d = start.elapsed();
            black_box(r.unwrap().unwrap());
            d
        })
        .unwrap()
    })
}

fn compare(names: &[&str], variants: &[fn() -> Duration]) {
    let mut buf = vec![0_u8; EVICT_BYTES];
    let mut times = vec![Vec::new(); variants.len()];
    for round in 0..ROUNDS {
        for (v, t) in variants.iter().zip(&mut times) {
            evict(&mut buf, round);
            t.push(v());
        }
    }
    for (n, t) in names.iter().zip(&times) {
        println!("{n:<28} {:>10.1} us", fastest(t));
    }
}

fn update_setup() -> (generation::Topology, generation::Field<f32>, generation::Field<f32>) {
    let ops: Vec<Op> = (0..LEAVES).map(|i| Op::Activate(leaf_region(i).map(|c| c * 8))).collect();
    let (t, _) = generation::Topology::new().commit(&ops).unwrap();
    let mut read = generation::Field::new(&t, 0.0_f32);
    for (k, x) in read.data_mut(&t).unwrap().leaf_values.iter_mut().enumerate() {
        *x = (k % 977) as f32;
    }
    let out = generation::Field::new(&t, 0.0_f32);
    (t, read, out)
}

fn kernel(read: &FieldData<f32>, leaf: u32, out: &mut [f32]) {
    for (o, r) in out.iter_mut().zip(read.leaf(leaf)) {
        *o = r * 0.5 + 1.0;
    }
}

fn update_unchecked() -> Duration {
    let (t, read, mut out) = update_setup();
    let r = read.data(&t).unwrap();
    let start = Instant::now();
    generation::par_update_unchecked(&mut out, |i, o| kernel(r, i, o));
    let d = start.elapsed();
    black_box(out);
    d
}

fn update_once() -> Duration {
    let (t, read, mut out) = update_setup();
    let start = Instant::now();
    let r = read.data(&t).unwrap();
    generation::par_update(&t, &mut out, |i, o| kernel(r, i, o)).unwrap();
    let d = start.elapsed();
    black_box(out);
    d
}

fn update_per_leaf() -> Duration {
    let (t, read, mut out) = update_setup();
    let start = Instant::now();
    generation::par_update_checked_per_leaf(&t, &read, &mut out, kernel).unwrap();
    let d = start.elapsed();
    black_box(out);
    d
}

fn main() {
    let which = std::env::args().nth(1).unwrap_or_default();
    println!("{which}: fastest of {ROUNDS}, {LEAVES} leaves x {LEAF_VOXELS} voxels");
    match which.as_str() {
        "remap" => compare(
            &["A: commit + 3 remaps", "B: FieldSet::commit", "C: commit + 3 remaps"],
            &[remap_a, remap_b, remap_c],
        ),
        "update" => compare(
            &["A: no check", "A: check once", "A: check per leaf"],
            &[update_unchecked, update_once, update_per_leaf],
        ),
        _ => println!("usage: measure remap | update"),
    }
}
