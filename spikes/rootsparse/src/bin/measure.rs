//! Times and sizes the two forms on each layout, and brodo's `label` on the same keys.
//!
//!   ./target/release/measure            every layout
//!
//! A layout's keys are root regions; brodo's topology puts one leaf at each region's
//! origin. Each time is the median of five runs; memory is the peak heap the build adds.

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::time::{Duration, Instant};

use brodo::analysis::{Connectivity, label};
use brodo::field::Field;
use brodo::topology::Topology;
use rootsparse_spike::*;

struct Counting;
static NOW: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let n = NOW.fetch_add(l.size(), Relaxed) + l.size();
        PEAK.fetch_max(n, Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        NOW.fetch_sub(l.size(), Relaxed);
        unsafe { System.dealloc(p, l) }
    }
}

#[global_allocator]
static A: Counting = Counting;

/// The median time of five runs of `f`, and the peak heap the first adds.
fn measure<T>(mut f: impl FnMut() -> T) -> (Duration, usize, T) {
    let base = NOW.load(Relaxed);
    PEAK.store(base, Relaxed);
    let first = f();
    let peak = PEAK.load(Relaxed) - base;
    let mut times = vec![];
    for _ in 0..5 {
        let t = Instant::now();
        std::hint::black_box(f());
        times.push(t.elapsed());
    }
    times.sort();
    (times[2], peak, first)
}

/// A tiny xorshift, so that a layout is the same on every run.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// Root regions span 2¹⁹ along an axis of `i32` voxels; scattered keys stay well inside.
const SPAN: i64 = 1 << 18;

fn block(n: i64) -> BTreeSet<R> {
    let mut out = BTreeSet::new();
    for z in 0..n {
        for y in 0..n {
            for x in 0..n {
                out.insert([x, y, z]);
            }
        }
    }
    out
}

fn shell(n: i64) -> BTreeSet<R> {
    block(n).into_iter().filter(|c| c.iter().any(|v| *v == 0 || *v == n - 1)).collect()
}

fn diagonal(n: i64) -> BTreeSet<R> {
    (0..n).map(|i| [i * 3, i * 3, i * 3]).collect()
}

fn scattered(n: usize, seed: u64) -> BTreeSet<R> {
    let mut r = Rng(seed);
    let mut out = BTreeSet::new();
    while out.len() < n {
        let c = [0, 1, 2].map(|_| (r.next() % (2 * SPAN as u64)) as i64 - SPAN);
        out.insert(c);
    }
    out
}

fn topology(keys: &BTreeSet<R>) -> Topology {
    let empty = Topology::new();
    let mut e = empty.edit();
    for k in keys {
        e.activate(k.map(|c| (c << 12) as i32)).unwrap();
    }
    e.commit().unwrap().0
}

fn ms(d: Duration) -> String {
    format!("{:.3}", d.as_secs_f64() * 1e3)
}

fn mib(b: usize) -> String {
    format!("{:.2}", b as f64 / (1 << 20) as f64)
}

fn main() {
    let mut layouts: Vec<(String, BTreeSet<R>)> = vec![
        ("one key".into(), block(1)),
        ("block 2³".into(), block(2)),
        ("block 4³".into(), block(4)),
        ("block 8³".into(), block(8)),
        ("shell 12³".into(), shell(12)),
    ];
    for n in [10, 20, 40, 50, 100, 1000, 10_000] {
        layouts.push((format!("diagonal {n}"), diagonal(n)));
    }
    for n in [10, 20, 30, 40, 50, 100, 1000, 10_000, 100_000] {
        layouts.push((format!("scattered {n}"), scattered(n, 0x9e37_79b9_7f4a_7c15 ^ n as u64)));
    }
    println!(
        "| Layout | Keys | Conn | Dense cells | Dense ms | Dense MiB | Boxes | Sparse ms | Sparse MiB | label ms | label MiB |"
    );
    println!("| :--- | ---: | :--- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
    for (name, keys) in &layouts {
        for conn in [Conn::Six, Conn::TwentySix] {
            let (dt, dm, d) = measure(|| Dense::new(keys, conn));
            let (st, sm, s) = measure(|| Sparse::new(keys, conn));
            assert_eq!(s.boxes() == 0, keys.is_empty(), "{name}: the sparse form holds boxes");
            let t = topology(keys);
            assert_eq!(t.leaf_count(), keys.len(), "{name}: one leaf a key");
            let f = Field::new(&t, 1.0_f32);
            let bc = if conn == Conn::Six { Connectivity::Six } else { Connectivity::TwentySix };
            let (lt, lm, l) = measure(|| label(&t, &f, |v| v > 0.0, bc));
            let too_spread = matches!(&l, Err(brodo::error::AnalysisError::TooSpread));
            assert_eq!(d.is_none(), too_spread, "{name}: the dense form refuses as label does");
            let (dc, dt, dm) = match &d {
                Some(d) => (d.cells().to_string(), ms(dt), mib(dm)),
                None => ("refused".into(), "-".into(), "-".into()),
            };
            let (lt, lm) = match l {
                Ok(_) => (ms(lt), mib(lm)),
                Err(e) => (format!("{e:?}"), "-".into()),
            };
            println!(
                "| {name} | {} | {conn:?} | {dc} | {dt} | {dm} | {} | {} | {} | {lt} | {lm} |",
                keys.len(),
                s.boxes(),
                ms(st),
                mib(sm)
            );
        }
    }
}
