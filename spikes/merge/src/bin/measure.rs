//! Cost measurements (TESTING.md T5): the variants of a comparison run in turn, each after
//! evicting the caches, keeping the fastest of `ROUNDS`. One comparison per process:
//! `measure commit` (R1 against R2) or `measure remap` (M-a against M-b). `commit` also
//! merges all 32 768 regions, as leaves, into leaf-sized tiles: many small merges.
//!
//! Eight blocks of 16³ leaf-sized regions (brodo's lower nodes, 128³ voxels) merge into
//! block tiles, from two starting points: every region a tile (`tiles`, what S1 allows) and
//! half leaves, half tiles (`mixed`, S2 only). Every value is 1, so uniform rules accept.

use std::hint::black_box;
use std::time::{Duration, Instant};

use merge_spike::*;

const ROUNDS: usize = 20;
const DIM: i32 = 16;
const EVICT_BYTES: usize = 256 << 20;
const S2: Policy = Policy { merging: Merging::AtOnce, background: BackgroundInMerge::Refuse };

fn blocks() -> Vec<Block> {
    (0..8).map(|i| [i % 2, i / 2 % 2, i / 4]).collect()
}

/// The starting topology and field, and the merge edit.
fn setup(mixed: bool) -> (Topology, FieldData<f64>, Vec<Op>) {
    let fill: Vec<Op> = blocks()
        .into_iter()
        .flat_map(|b| children(b, DIM))
        .map(|r| if mixed && (r[0] + r[1] + r[2]) % 2 == 0 { Op::Activate(r) } else { Op::AddTile(r) })
        .collect();
    let (t, _) = commit_r2(&Topology::new(DIM), &fill, S2).unwrap();
    let mut f = FieldData::new(&t, 0.0);
    f.leaf_values.fill(1.0);
    f.tile_values.fill(1.0);
    (t, f, blocks().into_iter().map(Op::MergeBlock).collect())
}

fn evict(buf: &mut [u8], round: usize) {
    buf.fill(round as u8);
    black_box(&buf);
}

fn us(times: &[Duration]) -> f64 {
    times.iter().min().map_or(f64::NAN, |d| d.as_secs_f64() * 1e6)
}

fn compare(names: &[String], variants: &mut [Box<dyn FnMut() -> Duration + '_>]) {
    let mut buf = vec![0_u8; EVICT_BYTES];
    let mut times = vec![Vec::new(); variants.len()];
    for round in 0..ROUNDS {
        for (v, t) in variants.iter_mut().zip(&mut times) {
            evict(&mut buf, round);
            t.push(v());
        }
    }
    for (n, t) in names.iter().zip(&times) {
        println!("{n:<40} {:>10.1} us", us(t));
    }
}

fn commit_comparison() {
    let mut names = Vec::new();
    let mut variants: Vec<Box<dyn FnMut() -> Duration>> = Vec::new();
    for mixed in [false, true] {
        let scenario = if mixed { "mixed" } else { "tiles" };
        let (t, _, ops) = setup(mixed);
        let (_, r1) = commit_r1(&t, &ops, S2).unwrap();
        let (_, r2) = commit_r2(&t, &ops, S2).unwrap();
        println!("{scenario}: R1 holds {} bytes, R2 {} bytes", r1.bytes(), r2.bytes());
        let (ta, tb, oa, ob) = (t.clone(), t, ops.clone(), ops);
        names.push(format!("{scenario} commit R1"));
        variants.push(Box::new(move || {
            let s = Instant::now();
            black_box(commit_r1(&ta, &oa, S2).unwrap());
            s.elapsed()
        }));
        names.push(format!("{scenario} commit R2"));
        variants.push(Box::new(move || {
            let s = Instant::now();
            black_box(commit_r2(&tb, &ob, S2).unwrap());
            s.elapsed()
        }));
    }
    // Many small merges: every region a leaf, each merged into a leaf-sized tile.
    let fill: Vec<Op> = blocks().into_iter().flat_map(|b| children(b, DIM)).map(Op::Activate).collect();
    let (t, _) = commit_r2(&Topology::new(DIM), &fill, S2).unwrap();
    let ops: Vec<Op> = t.leaves().iter().map(|r| Op::MergeLeaf(*r)).collect();
    let (_, r1) = commit_r1(&t, &ops, S2).unwrap();
    let (_, r2) = commit_r2(&t, &ops, S2).unwrap();
    println!("leaves: R1 holds {} bytes, R2 {} bytes", r1.bytes(), r2.bytes());
    let (ta, tb, oa, ob) = (t.clone(), t, ops.clone(), ops);
    names.push("leaves commit R1".to_string());
    variants.push(Box::new(move || {
        let s = Instant::now();
        black_box(commit_r1(&ta, &oa, S2).unwrap());
        s.elapsed()
    }));
    names.push("leaves commit R2".to_string());
    variants.push(Box::new(move || {
        let s = Instant::now();
        black_box(commit_r2(&tb, &ob, S2).unwrap());
        s.elapsed()
    }));
    compare(&names, &mut variants);
}

fn remap_comparison() {
    let mut names = Vec::new();
    let mut variants: Vec<Box<dyn FnMut() -> Duration>> = Vec::new();
    let rules: [(&str, Rule<'static, f64>); 4] = [
        ("uniform M-a", Rule::Voxels(&uniform_voxels::<f64>)),
        ("uniform M-b", Rule::Counted(&uniform_counted::<f64>)),
        ("mean M-a", Rule::Voxels(&mean_voxels)),
        ("mean M-b", Rule::Counted(&mean_counted)),
    ];
    for mixed in [false, true] {
        let scenario = if mixed { "mixed" } else { "tiles" };
        let (t, f, ops) = setup(mixed);
        let (_, r) = commit_r2(&t, &ops, S2).unwrap();
        let shared = std::rc::Rc::new((f, r));
        for (name, rule) in rules {
            let s2 = std::rc::Rc::clone(&shared);
            names.push(format!("{scenario} remap {name}"));
            variants.push(Box::new(move || {
                let s = Instant::now();
                black_box(remap(&s2.0, &s2.1, rule).unwrap());
                s.elapsed()
            }));
        }
    }
    compare(&names, &mut variants);
}

fn main() {
    let which = std::env::args().nth(1).unwrap_or_default();
    println!("{which}: fastest of {ROUNDS}, 8 blocks of {DIM}^3 regions");
    match which.as_str() {
        "commit" => commit_comparison(),
        "remap" => remap_comparison(),
        _ => println!("usage: measure commit | remap"),
    }
}
