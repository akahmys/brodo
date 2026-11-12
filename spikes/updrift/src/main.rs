//! Spike for B-026 (ROADMAP.md §3): why is `leaf_parallel_update_x32` found 10 to 21 %
//! slower than its baseline in some sessions, both children of a pair alike, with the code
//! on its path unchanged?
//!
//! The parent runs children as the benchmark harness does, each on as many rayon threads as
//! the machine has performance cores, after the harness's rest. While a child runs, the
//! parent may spin threads of its own at a chosen QoS, standing for other work the load
//! averages let through. A child runs the benchmark's update (`plain`) and the same with
//! each leaf's thread, CPU and time recorded (`traced`) in turn for `ROUNDS` rounds, each
//! after a 256 MiB eviction, reads its own CPU time on each core type from
//! `proc_pid_rusage` around each run, and every CPU's busy time from `host_processor_info`
//! around the whole loop, so that the time other processes took on each core type is
//! seen.
//!
//! `updrift-spike PAIRS SPINS [QOS]` runs the parent: for each of `PAIRS` rounds, one child
//! for each count in `SPINS` (comma-separated), in an order turned each round;
//! `updrift-spike child PATH` runs one child.

use std::collections::BTreeSet;
use std::hint::black_box;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed};
use std::time::{Duration, Instant};

use brodo::field::Field;
use brodo::parallel::par_update;
use brodo::topology::Topology;
use brodo::tree::{Coord, TileLevel};

type Failure = Box<dyn std::error::Error + Send + Sync>;

const ROUNDS: usize = 20;
const EVICT_BYTES: usize = 256 << 20;
const RADIUS: i32 = 80;
const BAND: f32 = 3.0;
const CALLS: usize = 32;
/// The harness's rest: `BUSY_LOAD` and `SETTLED_LOAD` (ADR-0019).
const REST: (f64, f64) = (3.0, 4.0);

// ---- macOS: CPU time on each core type, every CPU's ticks, the current CPU, the clock ----

/// `struct rusage_info_v6` from `<sys/resource.h>`: 16 bytes of UUID, then `u64` fields,
/// read here by their place; the tail is room for fields a later macOS adds.
#[repr(C)]
struct RusageV6 {
    uuid: [u8; 16],
    f: [u64; 96],
}
const RI_USER_TIME: usize = 0;
const RI_SYSTEM_TIME: usize = 1;
const RI_INSTRUCTIONS: usize = 29;
const RI_CYCLES: usize = 30;
const RI_RUNNABLE_TIME: usize = 34;
const RI_USER_PTIME: usize = 36;
const RI_SYSTEM_PTIME: usize = 37;
const RI_PINSTRUCTIONS: usize = 38;
const RI_PCYCLES: usize = 39;
const RUSAGE_INFO_V6: i32 = 6;
/// `PROCESSOR_CPU_LOAD_INFO` from `<mach/processor_info.h>`: per CPU, `CPU_STATE_MAX` (4)
/// tick counts, user, system, idle and nice, in `CLK_TCK` (100) a second.
const PROCESSOR_CPU_LOAD_INFO: i32 = 2;
const CPU_STATE_IDLE: usize = 2;

unsafe extern "C" {
    fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut RusageV6) -> i32;
    fn pthread_cpu_number_np(cpu: *mut usize) -> i32;
    fn pthread_set_qos_class_self_np(qos: u32, relative: i32) -> i32;
    fn mach_absolute_time() -> u64;
    fn mach_timebase_info(info: *mut [u32; 2]) -> i32;
    fn getpid() -> i32;
    fn mach_host_self() -> u32;
    fn host_processor_info(
        host: u32,
        flavor: i32,
        count: *mut u32,
        info: *mut *mut [u32; 4],
        info_count: *mut u32,
    ) -> i32;
    fn vm_deallocate(task: u32, address: usize, size: usize) -> i32;
    static mach_task_self_: u32;
    fn task_info(task: u32, flavor: i32, info: *mut [i32; 8], count: *mut u32) -> i32;
}

/// `TASK_EVENTS_INFO` from `<mach/task_info.h>`: faults, page-ins, copy-on-write faults,
/// messages sent and received, Mach and Unix system calls, context switches. macOS leaves
/// `getrusage`'s `ru_minflt` at zero (measured), so the faults are read here.
const TASK_EVENTS_INFO: i32 = 2;

/// The process's page faults and context switches so far.
fn events() -> (i64, i64) {
    let (mut e, mut n) = ([0_i32; 8], 8_u32);
    // SAFETY: the buffer holds `TASK_EVENTS_INFO_COUNT` (8) words, as `n` says.
    let ok = unsafe { task_info(mach_task_self_, TASK_EVENTS_INFO, &mut e, &mut n) };
    assert_eq!(ok, 0, "task_info failed");
    (i64::from(e[0]), i64::from(e[7]))
}

static PLACE: u64 = 0;

fn cpu() -> u32 {
    let mut c = 0_usize;
    // SAFETY: writes one usize to a valid place.
    unsafe { pthread_cpu_number_np(&mut c) };
    c as u32
}

fn ticks() -> u64 {
    // SAFETY: no arguments.
    unsafe { mach_absolute_time() }
}

/// Nanoseconds per tick of `mach_absolute_time`.
fn tick_ns() -> f64 {
    let mut tb = [0_u32; 2];
    // SAFETY: writes two u32 to a valid place.
    unsafe { mach_timebase_info(&mut tb) };
    f64::from(tb[0]) / f64::from(tb[1])
}

/// Every CPU's busy ticks (user, system and nice), by its number, in hundredths of a second.
fn host_busy() -> Vec<u64> {
    let (mut n, mut info, mut words) = (0_u32, std::ptr::null_mut(), 0_u32);
    // SAFETY: the call writes the CPU count, an array it allocates of `n` entries of four
    // u32, and its length in u32; the array is read within that length and then freed.
    unsafe {
        let r = host_processor_info(mach_host_self(), PROCESSOR_CPU_LOAD_INFO, &mut n, &mut info, &mut words);
        assert_eq!(r, 0, "host_processor_info failed: {r}");
        assert_eq!(words, 4 * n);
        let cpus = std::slice::from_raw_parts(info, n as usize);
        let busy = cpus
            .iter()
            .map(|c| c.iter().enumerate().filter(|(k, _)| *k != CPU_STATE_IDLE).map(|(_, t)| u64::from(*t)).sum())
            .collect();
        vm_deallocate(mach_task_self_, info as usize, words as usize * 4);
        busy
    }
}

/// The process's counters: CPU time (raw units), on the performance cores, cycles.
#[derive(Clone, Copy, Default)]
struct Usage {
    time: u64,
    system: u64,
    ptime: u64,
    cycles: u64,
    pcycles: u64,
    instructions: u64,
    pinstructions: u64,
    runnable: u64,
}

fn usage() -> Usage {
    let mut r = RusageV6 { uuid: [0; 16], f: [0; 96] };
    // SAFETY: the buffer is larger than `rusage_info_v6`.
    let ok = unsafe { proc_pid_rusage(getpid(), RUSAGE_INFO_V6, &mut r) };
    assert_eq!(ok, 0, "proc_pid_rusage failed");
    let f = &r.f;
    Usage {
        time: f[RI_USER_TIME] + f[RI_SYSTEM_TIME],
        system: f[RI_SYSTEM_TIME],
        ptime: f[RI_USER_PTIME] + f[RI_SYSTEM_PTIME],
        cycles: f[RI_CYCLES],
        pcycles: f[RI_PCYCLES],
        instructions: f[RI_INSTRUCTIONS],
        pinstructions: f[RI_PINSTRUCTIONS],
        runnable: f[RI_RUNNABLE_TIME],
    }
}

impl Usage {
    fn since(self, a: Usage) -> Usage {
        Usage {
            time: self.time - a.time,
            system: self.system - a.system,
            ptime: self.ptime - a.ptime,
            cycles: self.cycles - a.cycles,
            pcycles: self.pcycles - a.pcycles,
            instructions: self.instructions - a.instructions,
            pinstructions: self.pinstructions - a.pinstructions,
            runnable: self.runnable - a.runnable,
        }
    }
}

/// The efficiency CPUs, as the IORegistry's `cluster-type` names them.
fn efficiency_cpus() -> Result<BTreeSet<u32>, Failure> {
    let out = Command::new("ioreg").args(["-rlc", "IOPlatformDevice", "-d1"]).output()?;
    let text = String::from_utf8(out.stdout)?;
    let mut e = BTreeSet::new();
    // Each CPU's properties come as one block of lines, in no fixed order.
    for block in text.split("+-o ") {
        let field = |key: &str| block.lines().find(|l| l.contains(&format!("\"{key}\""))).map(|l| l.to_owned());
        if let (Some(kind), Some(id)) = (field("cluster-type"), field("logical-cpu-id")) {
            if kind.contains("<\"E\">") {
                e.insert(id.rsplit(' ').next().unwrap_or("").parse()?);
            }
        }
    }
    Ok(e)
}

/// Checks that `cpu()` and `host_busy()` number CPUs as the IORegistry does: one thread
/// spun at the default QoS, the share of its samples on the performance CPUs against the
/// share of its CPU time `proc_pid_rusage` puts there.
fn check_cpu_numbers(e: &BTreeSet<u32>) -> Result<String, Failure> {
    let a = usage();
    let e2 = e.clone();
    let (on_p, all) = std::thread::spawn(move || {
        let (mut on_p, mut all) = (0_u64, 0_u64);
        let end = Instant::now() + Duration::from_millis(300);
        while Instant::now() < end {
            all += 1;
            on_p += u64::from(!e2.contains(&cpu()));
        }
        (on_p, all)
    })
    .join()
    .map_err(|_| "the probe thread panicked")?;
    let u = usage().since(a);
    let sampled = on_p as f64 / all as f64;
    let counted = u.ptime as f64 / u.time as f64;
    let report = format!(" sampled_p {sampled:.3} rusage_p {counted:.3}");
    assert!((sampled - counted).abs() < 0.1, "CPU numbers disagree with rusage:{report}");
    Ok(report)
}

/// The units of the rusage times, in nanoseconds, from one thread spun for 200 ms.
fn rusage_unit() -> f64 {
    let a = usage();
    let end = Instant::now() + Duration::from_millis(200);
    let mut x = 0_u64;
    while Instant::now() < end {
        x = black_box(x.wrapping_add(1));
    }
    let d = usage().since(a).time as f64;
    // Either nanoseconds or ticks; take whichever puts the spin nearer 200 ms.
    let candidates = [1.0, tick_ns()];
    let best = candidates
        .into_iter()
        .min_by(|p, q| ((d * p / 2e8).ln().abs()).total_cmp(&(d * q / 2e8).ln().abs()))
        .unwrap_or(1.0);
    let spun = d * best / 1e6;
    assert!((150.0..300.0).contains(&spun), "a 200 ms spin read as {spun:.1} ms");
    best
}

fn sysctl(name: &str) -> Result<String, Failure> {
    let out = Command::new("sysctl").args(["-n", name]).output()?;
    Ok(String::from_utf8(out.stdout)?.trim().to_owned())
}

// ---- The workload, as the benchmark builds it ----

fn distance(v: Coord) -> f32 {
    let [x, y, z] = v.map(f64::from);
    ((x * x + y * y + z * z).sqrt() - f64::from(RADIUS)) as f32
}

fn sphere() -> Result<(Topology, Field<f32>), Failure> {
    let lim = RADIUS + 16;
    let (mut leaves, mut tiles) = (Vec::new(), Vec::new());
    for z in (-lim..lim).step_by(8) {
        for y in (-lim..lim).step_by(8) {
            for x in (-lim..lim).step_by(8) {
                let ds = (0..512).map(|i| distance([x + i % 8, y + i / 8 % 8, z + i / 64]));
                let (lo, hi) = ds.fold((f32::MAX, f32::MIN), |(lo, hi), d| (lo.min(d), hi.max(d)));
                if hi <= -BAND {
                    tiles.push([x, y, z]);
                } else if lo < BAND {
                    leaves.push([x, y, z]);
                }
            }
        }
    }
    let empty = Topology::new();
    let mut e = empty.edit();
    for v in &leaves {
        e.activate(*v)?;
    }
    for v in &tiles {
        e.add_tile(TileLevel::Lower, *v)?;
    }
    let t = e.commit()?.0;
    let mut f = Field::new(&t, BAND);
    par_update(&t, &mut f, |n, values| -> Result<(), Failure> {
        for (voxel, v) in t.leaf_voxels(n)?.zip(values.iter_mut()) {
            *v = distance(voxel).clamp(-BAND, BAND);
        }
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    for n in t.tile_numbers().collect::<Vec<_>>() {
        f.set_tile(&t, n, -BAND)?;
    }
    Ok((t, f))
}

fn update(t: &Topology, f: &Field<f32>) -> Result<f64, Failure> {
    let mut out = Field::new(t, 0.0_f32);
    update_into(t, f, &mut out)
}

/// The update into a field already made, whose pages are already touched.
fn update_into(t: &Topology, f: &Field<f32>, out: &mut Field<f32>) -> Result<f64, Failure> {
    par_update(t, out, |n, values| -> Result<(), Failure> {
        for (o, v) in values.iter_mut().zip(f.leaf(t, n)?) {
            *o = 2.0 * v + 1.0;
        }
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    Ok(f64::from(out.background()))
}

/// Each leaf's thread, CPU, and start and end in ticks, for one call.
struct Trace {
    thread: Vec<AtomicU32>,
    cpu: Vec<AtomicU32>,
    start: Vec<AtomicU64>,
    end: Vec<AtomicU64>,
}

impl Trace {
    fn new(n: usize) -> Self {
        Self {
            thread: (0..n).map(|_| AtomicU32::new(u32::MAX)).collect(),
            cpu: (0..n).map(|_| AtomicU32::new(0)).collect(),
            start: (0..n).map(|_| AtomicU64::new(0)).collect(),
            end: (0..n).map(|_| AtomicU64::new(0)).collect(),
        }
    }
}

fn traced_update(t: &Topology, f: &Field<f32>, tr: &Trace) -> Result<(f64, u64, u64), Failure> {
    let mut out = Field::new(t, 0.0_f32);
    let begin = ticks();
    par_update(t, &mut out, |n, values| -> Result<(), Failure> {
        let i = n.get() as usize;
        tr.start[i].store(ticks(), Relaxed);
        tr.thread[i].store(rayon::current_thread_index().map_or(u32::MAX - 1, |k| k as u32), Relaxed);
        tr.cpu[i].store(cpu(), Relaxed);
        for (o, v) in values.iter_mut().zip(f.leaf(t, n)?) {
            *o = 2.0 * v + 1.0;
        }
        tr.end[i].store(ticks(), Relaxed);
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    let finish = ticks();
    Ok((f64::from(out.background()), begin, finish))
}

/// What one call's trace shows, summed over the calls of a run.
#[derive(Default)]
struct Shape {
    leaves: u64,
    e_leaves: u64,
    p_busy: u64,
    e_busy: u64,
    threads: u64,
    /// Per call: the last thread's first leaf after the call began.
    late_start: u64,
    /// Per call: the call's end after its last leaf ended.
    tail: u64,
    /// Per call: the last thread's last leaf after the first thread's last leaf.
    spread: u64,
}

fn shape(tr: &Trace, e: &BTreeSet<u32>, begin: u64, finish: u64, s: &mut Shape) {
    let nthreads = rayon::current_num_threads();
    let mut first = vec![u64::MAX; nthreads];
    let mut last = vec![0_u64; nthreads];
    for i in 0..tr.thread.len() {
        let k = tr.thread[i].load(Relaxed) as usize;
        assert!(k < nthreads, "leaf {i} ran outside the pool");
        let (a, b) = (tr.start[i].load(Relaxed), tr.end[i].load(Relaxed));
        first[k] = first[k].min(a);
        last[k] = last[k].max(b);
        s.leaves += 1;
        if e.contains(&tr.cpu[i].load(Relaxed)) {
            s.e_leaves += 1;
            s.e_busy += b - a;
        } else {
            s.p_busy += b - a;
        }
    }
    let used: Vec<usize> = (0..nthreads).filter(|k| first[*k] != u64::MAX).collect();
    s.threads += used.len() as u64;
    let max_first = used.iter().map(|k| first[*k]).max().unwrap_or(begin);
    let max_last = used.iter().map(|k| last[*k]).max().unwrap_or(begin);
    let min_last = used.iter().map(|k| last[*k]).min().unwrap_or(begin);
    s.late_start += max_first - begin;
    s.tail += finish - max_last;
    s.spread += max_last - min_last;
}

// ---- The child ----

fn evict(buffer: &mut [u8]) {
    for (i, b) in (0_u8..=255).cycle().zip(buffer.iter_mut().step_by(64)) {
        *b = b.wrapping_add(i);
    }
    black_box(&*buffer);
}

fn child(path: &str) -> Result<(), Failure> {
    let threads: usize = sysctl("hw.perflevel0.logicalcpu")?.parse()?;
    rayon::ThreadPoolBuilder::new().num_threads(threads).build_global()?;
    let e = efficiency_cpus()?;
    let ncpu: usize = sysctl("hw.ncpu")?.parse()?;
    let ne: usize = sysctl("hw.perflevel1.logicalcpu")?.parse()?;
    assert_eq!(e.len(), ne, "the IORegistry names {e:?} efficiency CPUs, not {ne}");
    assert_eq!(host_busy().len(), ncpu, "host_processor_info counts other CPUs than hw.ncpu");
    let unit = rusage_unit();
    let checked = check_cpu_numbers(&e)?;
    let tick = tick_ns();
    let (t, f) = sphere()?;
    let n = t.leaf_count();
    let traces: Vec<Trace> = (0..CALLS).map(|_| Trace::new(n)).collect();
    let mut buffer = vec![0_u8; EVICT_BYTES];
    let mut kept = Field::new(&t, 0.0_f32);
    update_into(&t, &f, &mut kept)?;
    let memory = format!(
        "# memory memorystatus_level {} page_free_count {} page_speculative_count {} pageable_internal {}\n",
        sysctl("kern.memorystatus_level")?,
        sysctl("vm.page_free_count")?,
        sysctl("vm.page_speculative_count")?,
        sysctl("vm.page_pageable_internal_count")?
    );
    let mut rows = memory;
    rows.push_str(&format!(
        "# cpus {ncpu} efficiency {e:?} rusage_unit_ns {unit:.3} tick_ns {tick:.3} leaves {n} threads {}{checked} code {:p} static {:p} field {:p}\n",
        rayon::current_num_threads(),
        child as fn(&str) -> Result<(), Failure>,
        &PLACE,
        f.leaf(&t, t.leaf_numbers().next().ok_or("no leaf")?)?.as_ptr(),
    ));
    rows.push_str("kind\tround\tms\tcpu_ms\tp_share\tp_ipc\tp_ghz\tinstr_m\twait_ms\te_leaves\tp_ns_leaf\te_ns_leaf\tthreads\tlate_start_us\ttail_us\tspread_us\tsys_ms\tfaults\tcsw\n");
    let (host0, own0, wall0) = (host_busy(), usage(), Instant::now());
    for round in 0..ROUNDS {
        let kinds = ["plain", "reused", "traced", "phased"];
        for kind in (0..4).map(|j| kinds[(round + j) % 4]) {
            evict(&mut buffer);
            let mut s = Shape::default();
            let (a, ev) = (usage(), events());
            let start = Instant::now();
            if kind == "plain" {
                for _ in 0..CALLS {
                    black_box(update(&t, &f)?);
                }
            } else if kind == "phased" {
                for _ in 0..CALLS {
                    let a = ticks();
                    let mut out = Field::new(&t, 0.0_f32);
                    let b = ticks();
                    black_box(update_into(&t, &f, &mut out)?);
                    let c = ticks();
                    drop(black_box(out));
                    let d = ticks();
                    s.p_busy += b - a;
                    s.e_busy += c - b;
                    s.spread += d - c;
                }
            } else if kind == "reused" {
                for _ in 0..CALLS {
                    black_box(update_into(&t, &f, &mut kept)?);
                }
            } else {
                let mut ends = Vec::with_capacity(CALLS);
                for tr in &traces {
                    let (v, b, fin) = traced_update(&t, &f, tr)?;
                    black_box(v);
                    ends.push((b, fin));
                }
                for (tr, (b, fin)) in traces.iter().zip(ends) {
                    shape(tr, &e, b, fin, &mut s);
                }
            }
            let ms = start.elapsed().as_secs_f64() * 1e3;
            let (u, ev2) = (usage().since(a), events());
            let (faults, csw) = (ev2.0 - ev.0, ev2.1 - ev.1);
            let time = u.time as f64 * unit;
            let per = |x: u64| x as f64 / CALLS as f64;
            rows.push_str(&format!(
                "{kind}\t{round}\t{ms:.3}\t{:.3}\t{:.3}\t{:.2}\t{:.2}\t{:.2}\t{:.3}\t{:.3}\t{:.1}\t{:.1}\t{:.2}\t{:.1}\t{:.1}\t{:.1}\t{:.3}\t{faults}\t{csw}\n",
                time / 1e6,
                u.ptime as f64 * unit / time,
                u.pinstructions as f64 / u.pcycles.max(1) as f64,
                u.pcycles as f64 / (u.ptime as f64 * unit),
                u.instructions as f64 / 1e6,
                (u.runnable as f64 * unit - time) / 1e6,
                s.e_leaves as f64 / s.leaves.max(1) as f64,
                s.p_busy as f64 * tick / (s.leaves - s.e_leaves).max(1) as f64,
                s.e_busy as f64 * tick / s.e_leaves.max(1) as f64,
                per(s.threads),
                per(s.late_start) * tick / 1e3,
                per(s.tail) * tick / 1e3,
                per(s.spread) * tick / 1e3,
                u.system as f64 * unit / 1e6,
            ));
        }
    }
    let wall = wall0.elapsed().as_secs_f64() * 1e3;
    let (host, own) = (host_busy(), usage().since(own0));
    let (mut p_busy, mut e_busy) = (0.0, 0.0);
    for (k, (b, a)) in host.iter().zip(&host0).enumerate() {
        let ms = (b - a) as f64 * 10.0;
        if e.contains(&(k as u32)) { e_busy += ms } else { p_busy += ms }
    }
    let own_p = own.ptime as f64 * unit / 1e6;
    let own_e = own.time as f64 * unit / 1e6 - own_p;
    rows.push_str(&format!(
        "# host wall_ms {wall:.0} p_busy_ms {p_busy:.0} e_busy_ms {e_busy:.0} own_p_ms {own_p:.0} own_e_ms {own_e:.0} others_p_ms {:.0} others_e_ms {:.0}\n",
        p_busy - own_p,
        e_busy - own_e
    ));
    std::fs::write(path, rows)?;
    Ok(())
}

// ---- The parent ----

fn load() -> Result<(f64, f64), Failure> {
    let s = sysctl("vm.loadavg")?;
    let v: Vec<f64> = s.trim_matches(['{', '}', ' ']).split_whitespace().filter_map(|x| x.parse().ok()).collect();
    Ok((v[0], v[1]))
}

fn wait_for_rest(p: usize) -> Result<(f64, f64), Failure> {
    loop {
        let (one, five) = load()?;
        if one <= REST.0 && five <= REST.1 {
            return Ok((one, five));
        }
        eprintln!("round {p}: waiting, load {one:.2} {five:.2}");
        std::thread::sleep(Duration::from_secs(10));
    }
}

/// Starts `n` threads spinning at `qos` until the flag is set; they stand for other work.
fn spin(n: usize, qos: u32, stop: &Arc<AtomicBool>) -> Vec<std::thread::JoinHandle<u64>> {
    (0..n)
        .map(|_| {
            let stop = Arc::clone(stop);
            std::thread::spawn(move || {
                // SAFETY: changes only this thread's QoS.
                unsafe { pthread_set_qos_class_self_np(qos, 0) };
                let mut x = 1_u64;
                while !stop.load(Relaxed) {
                    for _ in 0..1000 {
                        x ^= x << 13;
                        x ^= x >> 7;
                        x ^= x << 17;
                    }
                    black_box(x);
                }
                x
            })
        })
        .collect()
}

/// A child's summary: its fastest and median `plain` runs, and the fastest `traced` run's
/// leaves on the efficiency cores and wait, with its last line.
fn summary(text: &str) -> String {
    let rows: Vec<Vec<&str>> = text.lines().filter(|l| !l.starts_with('#') && !l.starts_with("kind")).map(|l| l.split('\t').collect()).collect();
    let ms = |r: &Vec<&str>| r[2].parse::<f64>().unwrap_or(f64::MAX);
    let mut plain: Vec<f64> = rows.iter().filter(|r| r[0] == "plain").map(ms).collect();
    plain.sort_by(f64::total_cmp);
    let mut reused: Vec<f64> = rows.iter().filter(|r| r[0] == "reused").map(ms).collect();
    reused.sort_by(f64::total_cmp);
    let traced = rows.iter().filter(|r| r[0] == "traced").min_by(|a, b| ms(a).total_cmp(&ms(b)));
    let fastest_plain = rows.iter().filter(|r| r[0] == "plain").min_by(|a, b| ms(a).total_cmp(&ms(b)));
    let host = text.lines().last().unwrap_or("").trim_start_matches("# host ");
    format!(
        "plain_min {:.3} plain_med {:.3} reused_min {:.3} p_share {} p_ipc {} p_ghz {} sys_ms {} faults {} csw {} | traced_min {} e_leaves {} wait_ms {} late_us {} | {host}",
        plain[0],
        plain[plain.len() / 2],
        reused[0],
        fastest_plain.map_or("-", |r| r[4]),
        fastest_plain.map_or("-", |r| r[5]),
        fastest_plain.map_or("-", |r| r[6]),
        fastest_plain.map_or("-", |r| r[16]),
        fastest_plain.map_or("-", |r| r[17]),
        fastest_plain.map_or("-", |r| r[18]),
        traced.map_or("-", |r| r[2]),
        traced.map_or("-", |r| r[9]),
        traced.map_or("-", |r| r[8]),
        traced.map_or("-", |r| r[13]),
    )
}

fn parent(rounds: usize, spins: &[usize], qos: u32) -> Result<(), Failure> {
    let me = std::env::current_exe()?;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/spike/updrift");
    std::fs::create_dir_all(&dir)?;
    let unit = rusage_unit();
    println!("# qos {qos:#x} spins {spins:?}");
    for p in 0..rounds {
        for k in 0..spins.len() {
            let n = spins[(p + k) % spins.len()];
            let (one, five) = wait_for_rest(p)?;
            let path = dir.join(format!("child-{p}-s{n}-q{qos:x}.tsv"));
            let stop = Arc::new(AtomicBool::new(false));
            let a = usage();
            let started = Instant::now();
            let spinners = spin(n, qos, &stop);
            let ok = Command::new(&me).arg("child").arg(&path).status()?.success();
            stop.store(true, Relaxed);
            for s in spinners {
                black_box(s.join().map_err(|_| "a spinner panicked")?);
            }
            let wall = started.elapsed().as_secs_f64();
            let u = usage().since(a);
            assert!(ok, "child {p} with {n} spinners failed");
            let text = std::fs::read_to_string(&path)?;
            println!(
                "{p}\tspin {n}\tload {one:.2} {five:.2}\tspin_cpu {:.2}\t{}",
                u.time as f64 * unit / 1e9 / wall,
                summary(&text)
            );
        }
    }
    Ok(())
}

/// The update's serial step alone: a zeroed vector of the output field's size made and
/// dropped 2000 times, `gap_us` of spinning between, with `spinners` busy threads of this
/// process beside it; prints the fastest and mean time of one.
fn zero(spinners: usize, gap_us: u64) -> Result<(), Failure> {
    const N: usize = 2000;
    let (t, _) = sphere()?;
    let len = t.leaf_count() * 512;
    let stop = Arc::new(AtomicBool::new(false));
    let threads = spin(spinners, 0x15, &stop);
    std::thread::sleep(Duration::from_millis(200));
    let (mut best, mut sum, mut faults) = (f64::MAX, 0.0, 0);
    for _ in 0..N {
        let ev = events();
        let start = Instant::now();
        drop(black_box(vec![0.0_f32; len]));
        let us = start.elapsed().as_secs_f64() * 1e6;
        faults += events().0 - ev.0;
        best = best.min(us);
        sum += us;
        let until = Instant::now() + Duration::from_micros(gap_us);
        while Instant::now() < until {}
    }
    stop.store(true, Relaxed);
    for s in threads {
        black_box(s.join().map_err(|_| "a spinner panicked")?);
    }
    println!("zero spinners {spinners} gap_us {gap_us} bytes {} best_us {best:.1} mean_us {:.1} faults {faults}", len * 4, sum / N as f64);
    Ok(())
}

fn main() -> Result<(), Failure> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("child") => child(&args[2]),
        Some("zero") => zero(args[2].parse()?, args[3].parse()?),
        Some(p) => {
            let spins: Vec<usize> =
                args.get(2).map_or("0", String::as_str).split(',').map(str::parse).collect::<Result<_, _>>()?;
            let qos = args.get(3).map_or(Ok(0x15), |q| u32::from_str_radix(q.trim_start_matches("0x"), 16))?;
            parent(p.parse()?, &spins, qos)
        }
        None => parent(3, &[0], 0x15),
    }
}
