//! Spike for B-024 (ROADMAP.md §3): why do the leaf-parallel update and colino's
//! `convert_slab_320` vary from one benchmark process to the next?
//!
//! The parent runs pairs of child processes, as the benchmark harness does, waiting for the
//! machine to rest before each pair. A child runs three kinds in turn for `ROUNDS` rounds,
//! each after a 256 MiB eviction: the benchmark's `leaf_parallel_update_x32` as it is
//! (`plain`), the same with each leaf's thread, CPU and time recorded (`traced`), and
//! `convert_slab_320` (`slab`). Around each run it reads the process's CPU time, and the part
//! of it spent on the performance cores, from `proc_pid_rusage`. The parent prints each
//! child's fastest run of each kind.
//!
//! `parvar-spike PAIRS` runs the parent; `parvar-spike child PATH` one child.

use std::collections::BTreeSet;
use std::hint::black_box;
use std::process::Command;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};
use std::time::{Duration, Instant};

use brodo::field::Field;
use brodo::parallel::par_update;
use brodo::topology::Topology;
use brodo::tree::{Coord, TileLevel};
use colino::convert::{Options, convert};
use colino::mesh::Mesh;

type Failure = Box<dyn std::error::Error + Send + Sync>;

const ROUNDS: usize = 20;
const EVICT_BYTES: usize = 256 << 20;
const RADIUS: i32 = 80;
const BAND: f32 = 3.0;
const CALLS: usize = 32;
/// The rest the parent waits for: the one- and five-minute load averages.
const REST: (f64, f64) = (2.5, 4.0);

// ---- macOS: CPU time on each core type, the current CPU, the clock ----

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
const QOS_CLASS_BACKGROUND: u32 = 0x09;

unsafe extern "C" {
    fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut RusageV6) -> i32;
    fn pthread_cpu_number_np(cpu: *mut usize) -> i32;
    fn pthread_set_qos_class_self_np(qos: u32, relative: i32) -> i32;
    fn mach_absolute_time() -> u64;
    fn mach_timebase_info(info: *mut [u32; 2]) -> i32;
    fn getpid() -> i32;
    fn posix_spawnattr_init(attr: *mut *mut u8) -> i32;
    fn posix_spawnattr_setflags(attr: *mut *mut u8, flags: i16) -> i32;
    fn posix_spawn(
        pid: *mut i32,
        path: *const std::ffi::c_char,
        actions: *const u8,
        attr: *const *mut u8,
        argv: *const *const std::ffi::c_char,
        envp: *const *const std::ffi::c_char,
    ) -> i32;
    fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
    static environ: *const *const std::ffi::c_char;
}

/// `_POSIX_SPAWN_DISABLE_ASLR` from `<spawn.h>`'s private flags, as lldb passes it.
const POSIX_SPAWN_DISABLE_ASLR: i16 = 0x0100;

/// Runs `exe child path` with address space layout randomisation on or off, and waits.
fn spawn_child(exe: &std::path::Path, path: &std::path::Path, aslr: bool) -> Result<(), Failure> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let args = [
        CString::new(exe.as_os_str().as_bytes())?,
        CString::new("child")?,
        CString::new(path.as_os_str().as_bytes())?,
    ];
    let mut argv: Vec<*const std::ffi::c_char> = args.iter().map(|a| a.as_ptr()).collect();
    argv.push(std::ptr::null());
    let mut attr: *mut u8 = std::ptr::null_mut();
    let (mut pid, mut status) = (0_i32, 0_i32);
    // SAFETY: the attribute is initialised before use; argv is null-terminated and its
    // strings outlive the call; environ is the process's own environment.
    unsafe {
        assert_eq!(posix_spawnattr_init(&mut attr), 0);
        if !aslr {
            assert_eq!(posix_spawnattr_setflags(&mut attr, POSIX_SPAWN_DISABLE_ASLR), 0);
        }
        let r = posix_spawn(&mut pid, args[0].as_ptr(), std::ptr::null(), &attr, argv.as_ptr(), environ);
        assert_eq!(r, 0, "posix_spawn failed: {r}");
        assert_eq!(waitpid(pid, &mut status, 0), pid);
    }
    assert_eq!(status, 0, "child {} failed: status {status}", path.display());
    Ok(())
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
        let field = |key: &str| {
            block.lines().find(|l| l.contains(&format!("\"{key}\""))).map(|l| l.to_owned())
        };
        if let (Some(kind), Some(id)) = (field("cluster-type"), field("logical-cpu-id")) {
            if kind.contains("<\"E\">") {
                e.insert(id.rsplit(' ').next().unwrap_or("").parse()?);
            }
        }
    }
    Ok(e)
}

/// Checks that `cpu()` numbers CPUs as the IORegistry does: one thread spun at each QoS,
/// the share of its samples on the performance CPUs against the share of its CPU time
/// that `proc_pid_rusage` puts on the performance cores.
fn check_cpu_numbers(e: &BTreeSet<u32>) -> Result<String, Failure> {
    let mut report = String::new();
    for (name, qos) in [("default", 0x15_u32), ("background", QOS_CLASS_BACKGROUND)] {
        let a = usage();
        let e2 = e.clone();
        let (on_p, all) = std::thread::spawn(move || {
            // SAFETY: changes only this thread's QoS.
            unsafe { pthread_set_qos_class_self_np(qos, 0) };
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
        report.push_str(&format!(" {name}: sampled_p {sampled:.3} rusage_p {counted:.3}"));
        assert!((sampled - counted).abs() < 0.1, "CPU numbers disagree with rusage:{report}");
    }
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

// ---- The workloads, as the benchmarks build them ----

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

fn slab(size: [f64; 3]) -> Mesh {
    let (a, b) = (0.5_f64, 0.3_f64);
    let turn = |corner: [f64; 3]| {
        let [x, y, z] = [0, 1, 2].map(|k| (corner[k] - 0.5) * size[k]);
        let (y, z) = (y * a.cos() - z * a.sin(), y * a.sin() + z * a.cos());
        let (x, y) = (x * b.cos() - y * b.sin(), x * b.sin() + y * b.cos());
        [x + 0.3, y + 0.1, z - 0.2]
    };
    let vertices = (0..8).map(|i| turn([0, 1, 2].map(|k| f64::from(i >> k & 1)))).collect();
    let triangles = vec![
        [0, 2, 3],
        [0, 3, 1],
        [4, 5, 7],
        [4, 7, 6],
        [0, 1, 5],
        [0, 5, 4],
        [2, 6, 7],
        [2, 7, 3],
        [0, 4, 6],
        [0, 6, 2],
        [1, 3, 7],
        [1, 7, 5],
    ];
    Mesh::new(vertices, triangles).with_unit(Some("millimeter".into()))
}

fn update(t: &Topology, f: &Field<f32>) -> Result<f64, Failure> {
    let mut out = Field::new(t, 0.0_f32);
    par_update(t, &mut out, |n, values| -> Result<(), Failure> {
        for (o, v) in values.iter_mut().zip(f.leaf(t, n)?) {
            *o = 2.0 * v + 1.0;
        }
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    Ok(f64::from(out.background()))
}

/// Work that touches no memory: four xorshift chains in registers, in 4096 parallel
/// pieces, as a control for the slab's loss of instructions per cycle.
fn alu() -> u64 {
    use rayon::prelude::*;
    (0..4096_u64)
        .into_par_iter()
        .map(|i| {
            let mut x = [i + 1, i + 2, i + 3, i + 4];
            for _ in 0..40_000 {
                for v in &mut x {
                    *v ^= *v << 13;
                    *v ^= *v >> 7;
                    *v ^= *v << 17;
                }
            }
            x.iter().fold(0, |a, v| a ^ v)
        })
        .reduce(|| 0, |a, b| a ^ b)
}

/// Work that touches no memory but issues many instructions a cycle: sixteen independent
/// multiply-add chains and eight xorshift chains, in 4096 parallel pieces, so that the
/// control draws the power the slab does rather than waiting on one chain.
fn wide() -> f64 {
    use rayon::prelude::*;
    (0..4096_u64)
        .into_par_iter()
        .map(|i| {
            let mut f = [0.0_f64; 16];
            for (k, v) in f.iter_mut().enumerate() {
                *v = (i + k as u64) as f64;
            }
            let mut x = [i + 1, i + 2, i + 3, i + 4, i + 5, i + 6, i + 7, i + 8];
            for _ in 0..20_000 {
                for v in &mut f {
                    *v = v.mul_add(0.999_999_9, 1e-9);
                }
                for v in &mut x {
                    *v ^= *v << 13;
                    *v ^= *v >> 7;
                    *v ^= *v << 17;
                }
            }
            f.iter().sum::<f64>() + x.iter().fold(0, |a, v| a ^ v) as f64
        })
        .sum()
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
    e_threads: u64,
    /// Per call: the last thread's first leaf after the call began.
    late_start: u64,
    /// Per call: the call's end after its last leaf ended.
    tail: u64,
    /// Per call: the last thread's last leaf after the first thread's last leaf.
    spread: u64,
    /// Per call: the call's whole length.
    wall: u64,
}

fn shape(tr: &Trace, e: &BTreeSet<u32>, begin: u64, finish: u64, s: &mut Shape) {
    let nthreads = rayon::current_num_threads();
    let mut first = vec![u64::MAX; nthreads];
    let mut last = vec![0_u64; nthreads];
    let mut on_e = vec![(0_u64, 0_u64); nthreads];
    for i in 0..tr.thread.len() {
        let k = tr.thread[i].load(Relaxed) as usize;
        assert!(k < nthreads, "leaf {i} ran outside the pool");
        let (a, b) = (tr.start[i].load(Relaxed), tr.end[i].load(Relaxed));
        first[k] = first[k].min(a);
        last[k] = last[k].max(b);
        let is_e = e.contains(&tr.cpu[i].load(Relaxed));
        s.leaves += 1;
        on_e[k].1 += 1;
        if is_e {
            s.e_leaves += 1;
            s.e_busy += b - a;
            on_e[k].0 += 1;
        } else {
            s.p_busy += b - a;
        }
    }
    let used: Vec<usize> = (0..nthreads).filter(|k| first[*k] != u64::MAX).collect();
    s.threads += used.len() as u64;
    s.e_threads += used.iter().filter(|k| 2 * on_e[**k].0 > on_e[**k].1).count() as u64;
    let max_first = used.iter().map(|k| first[*k]).max().unwrap_or(begin);
    let max_last = used.iter().map(|k| last[*k]).max().unwrap_or(begin);
    let min_last = used.iter().map(|k| last[*k]).min().unwrap_or(begin);
    s.late_start += max_first - begin;
    s.tail += finish - max_last;
    s.spread += max_last - min_last;
    s.wall += finish - begin;
}

// ---- The child and the parent ----

fn evict(buffer: &mut [u8]) {
    for (i, b) in (0_u8..=255).cycle().zip(buffer.iter_mut().step_by(64)) {
        *b = b.wrapping_add(i);
    }
    black_box(&*buffer);
}

fn child(path: &str) -> Result<(), Failure> {
    let e = efficiency_cpus()?;
    let ncpu: u32 = sysctl("hw.ncpu")?.parse()?;
    let ne: usize = sysctl("hw.perflevel1.logicalcpu")?.parse()?;
    assert_eq!(e.len(), ne, "the IORegistry names {e:?} efficiency CPUs, not {ne}");
    let unit = rusage_unit();
    let checked = check_cpu_numbers(&e)?;
    let tick = tick_ns();
    let (t, f) = sphere()?;
    let n = t.leaf_count();
    let traces: Vec<Trace> = (0..CALLS).map(|_| Trace::new(n)).collect();
    let mesh = slab([320.0, 320.0, 6.0]);
    let mut buffer = vec![0_u8; EVICT_BYTES];
    let heap = Box::new(0_u64);
    let addresses = format!(
        "# addresses static {:p} code {:p} heap {:p} mesh {:p} buffer {:p}\n",
        &PLACE,
        child as fn(&str) -> Result<(), Failure>,
        &*heap,
        mesh.vertices.as_ptr(),
        buffer.as_ptr()
    );
    let mut rows = addresses;
    rows.push_str(&format!(
        "# cpus {ncpu} efficiency {e:?} rusage_unit_ns {unit:.3} tick_ns {tick:.3} leaves {n} threads {}{checked}\n",
        rayon::current_num_threads()
    ));
    rows.push_str("kind\tround\tms\tcpu_ms\tp_share\tp_ghz\te_ghz\tipc\trunnable_ms\te_leaves\tp_ns_leaf\te_ns_leaf\tthreads\te_threads\tlate_start_us\ttail_us\tspread_us\tcall_us\tp_instr_share\tp_ipc\te_ipc\tsys_ms\n");
    let wanted = std::env::var("PARVAR_KINDS").unwrap_or_else(|_| "plain,traced,slab".into());
    let kinds: Vec<&str> = wanted.split(',').collect();
    for round in 0..ROUNDS {
        for j in 0..kinds.len() {
            let kind = kinds[(round + j) % kinds.len()];
            evict(&mut buffer);
            let mut s = Shape::default();
            let a = usage();
            let start = Instant::now();
            match kind {
                "plain" => {
                    for _ in 0..CALLS {
                        black_box(update(&t, &f)?);
                    }
                }
                "traced" => {
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
                "alu" => {
                    black_box(alu());
                }
                "wide" => {
                    black_box(wide());
                }
                _ => {
                    black_box(convert(&mesh, &Options::new(1.0))?.topology.leaf_count());
                }
            }
            let ms = start.elapsed().as_secs_f64() * 1e3;
            let u = usage().since(a);
            let time = u.time as f64 * unit;
            let ptime = u.ptime as f64 * unit;
            let etime = time - ptime;
            let per = |x: u64| x as f64 / CALLS as f64;
            rows.push_str(&format!(
                "{kind}\t{round}\t{ms:.3}\t{:.3}\t{:.3}\t{:.2}\t{:.2}\t{:.2}\t{:.3}\t{:.3}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.3}\t{:.2}\t{:.2}\t{:.3}\n",
                time / 1e6,
                ptime / time,
                u.pcycles as f64 / ptime,
                (u.cycles - u.pcycles) as f64 / etime,
                u.instructions as f64 / u.cycles as f64,
                u.runnable as f64 * unit / 1e6,
                s.e_leaves as f64 / s.leaves.max(1) as f64,
                s.p_busy as f64 * tick / (s.leaves - s.e_leaves).max(1) as f64,
                s.e_busy as f64 * tick / s.e_leaves.max(1) as f64,
                per(s.threads),
                per(s.e_threads),
                per(s.late_start) * tick / 1e3,
                per(s.tail) * tick / 1e3,
                per(s.spread) * tick / 1e3,
                per(s.wall) * tick / 1e3,
                u.pinstructions as f64 / u.instructions as f64,
                u.pinstructions as f64 / u.pcycles as f64,
                (u.instructions - u.pinstructions) as f64 / (u.cycles - u.pcycles) as f64,
                u.system as f64 * unit / 1e6,
            ));
        }
    }
    std::fs::write(path, rows)?;
    Ok(())
}

/// Pairs of children, one with address space layout randomisation off and one with it on,
/// in alternating order; then checks that the children without it had one layout.
fn aslr(pairs: usize) -> Result<(), Failure> {
    let me = std::env::current_exe()?;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/spike/parvar");
    std::fs::create_dir_all(&dir)?;
    for p in 0..pairs {
        wait_for_rest(p)?;
        let order = if p % 2 == 0 { [false, true] } else { [true, false] };
        for on in order {
            let path = dir.join(format!("aslr-{p}-{}.tsv", if on { "on" } else { "off" }));
            spawn_child(&me, &path, on)?;
            let text = std::fs::read_to_string(&path)?;
            eprintln!("{p} {}: {}", if on { "on" } else { "off" }, text.lines().next().unwrap_or(""));
        }
    }
    let layouts = |on: &str| -> Result<BTreeSet<String>, Failure> {
        (0..pairs)
            .map(|p| {
                let text = std::fs::read_to_string(dir.join(format!("aslr-{p}-{on}.tsv")))?;
                // The image's places; the heap's are chosen apart from the image's (measured:
                // they differ between children without ASLR).
                let line = text.lines().next().unwrap_or("");
                Ok(line.split(" heap ").next().unwrap_or("").to_owned())
            })
            .collect()
    };
    let (off, on) = (layouts("off")?, layouts("on")?);
    println!("image layouts without ASLR {}, with it {}", off.len(), on.len());
    assert_eq!(off.len(), 1, "the children without ASLR had different image layouts: {off:?}");
    assert!(on.len() > 1, "the children with ASLR had one layout");
    Ok(())
}

fn wait_for_rest(p: usize) -> Result<(), Failure> {
    loop {
        let (one, five) = load()?;
        if one < REST.0 && five < REST.1 {
            eprintln!("pair {p}: load {one:.2} {five:.2}");
            return Ok(());
        }
        std::thread::sleep(Duration::from_secs(10));
    }
}

fn load() -> Result<(f64, f64), Failure> {
    let s = sysctl("vm.loadavg")?;
    let v: Vec<f64> = s.trim_matches(['{', '}', ' ']).split_whitespace().filter_map(|x| x.parse().ok()).collect();
    Ok((v[0], v[1]))
}

fn parent(pairs: usize) -> Result<(), Failure> {
    let me = std::env::current_exe()?;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/spike/parvar");
    std::fs::create_dir_all(&dir)?;
    for p in 0..pairs {
        loop {
            let (one, five) = load()?;
            if one < REST.0 && five < REST.1 {
                eprintln!("pair {p}: load {one:.2} {five:.2}");
                break;
            }
            eprintln!("pair {p}: waiting, load {one:.2} {five:.2}");
            std::thread::sleep(Duration::from_secs(10));
        }
        for c in 0..2 {
            let path = dir.join(format!("child-{p}-{c}.tsv"));
            let ok = Command::new(&me).arg("child").arg(&path).status()?.success();
            assert!(ok, "child {p}-{c} failed");
            let text = std::fs::read_to_string(&path)?;
            print_fastest(&format!("{p}-{c}"), &text);
        }
    }
    Ok(())
}

/// Prints the child's fastest run of each kind, the row whole.
fn print_fastest(name: &str, text: &str) {
    let mut lines = text.lines();
    if let Some(h) = lines.next() {
        if name == "0-0" {
            println!("{h}");
        }
    }
    let header = lines.next().unwrap_or_default();
    if name == "0-0" {
        println!("child\t{header}");
    }
    let rows: Vec<Vec<&str>> = lines.map(|l| l.split('\t').collect()).collect();
    for kind in ["plain", "traced", "slab"] {
        let best = rows
            .iter()
            .filter(|r| r[0] == kind)
            .min_by(|a, b| a[2].parse::<f64>().unwrap_or(f64::MAX).total_cmp(&b[2].parse().unwrap_or(f64::MAX)));
        if let Some(r) = best {
            println!("{name}\t{}", r.join("\t"));
        }
    }
}

fn main() -> Result<(), Failure> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("child") => child(&args[2]),
        Some("aslr") => aslr(args.get(2).map_or(Ok(10), |n| n.parse())?),
        Some(p) => parent(p.parse()?),
        None => parent(5),
    }
}
