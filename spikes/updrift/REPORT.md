# Spike: updrift (B-026)

Measured on 2026-11-12 with rustc 1.98.1, release build, on an Apple M5 (Mac17,2, macOS
27.0.1; four performance cores, six efficiency), on AC power with Low Power Mode off
(`pmset -g`). Every child started at a one-minute load of 1.58 to 3.00 and a five-minute
load of 1.49 to 2.06, within the harness's rest (ADR-0019). Reproduce from the repository's
root:

```bash
cargo build --release --manifest-path spikes/updrift/Cargo.toml
spikes/updrift/target/release/updrift-spike 5 0,1,2,3      # spinners at the default QoS
spikes/updrift/target/release/updrift-spike 3 0,1,2 9      # spinners at background QoS
spikes/updrift/target/release/updrift-spike 10 0           # at rest alone
spikes/updrift/target/release/updrift-spike zero 1 70      # the serial step alone
```

The parent prints each child's summary; every run's row is kept under
`out/spike/updrift/`.

## Question

**B-026**: why is `leaf_parallel_update_x32` found 10 to 21 % slower than its baseline in
some sessions (P7's and P9's closes, +10.4 and +5.7 % at P12's on 2026-11-11), both
children of a pair alike, with the code on its path unchanged (ADR-0019)?

## The harness

`src/main.rs` runs children as the benchmark harness does: each a fresh process on as many
rayon threads as the machine has performance cores (four), after the harness's rest, 20
rounds of each kind after a 256 MiB eviction, the fastest run kept. The kinds, each 32
calls on the benchmark's sphere (2606 leaves):

- `plain`: the benchmark's item as it is, a new output `Field` each call;
- `reused`: the same update into one output field made before the rounds, so that no call
  allocates;
- `phased`: `plain` with each call's `Field::new`, update and drop timed apart;
- `traced`: `plain` with each leaf's thread, CPU and time recorded.

Around each run a child reads its CPU time on each core type, cycles and instructions
from `proc_pid_rusage`, and its page faults and context switches from `task_info`
(`TASK_EVENTS_INFO`; macOS leaves `getrusage`'s `ru_minflt` at zero, measured); around
the whole loop, every CPU's busy time from `host_processor_info`. While a child runs, the
parent spins N threads of its own at a chosen QoS, standing for other work the load
averages let through; their CPU time, read from the parent's rusage, is N within 1 % in
every child. `zero` times the update's serial step alone, in one process with its own
spinners.

Before it times anything a child asserts that the IORegistry names as many efficiency CPUs
as `hw.perflevel1.logicalcpu`, that `host_processor_info` counts `hw.ncpu` CPUs, that a
200 ms spin reads as 150–300 ms of rusage time, and that a thread's samples of
`pthread_cpu_number_np` fall on the performance CPUs as often as rusage counts its time
there.

## Measurements

### The item beside other work

Fastest `plain` and `reused` run of each child, and the clock of the fastest `plain` run
on the performance cores (cycles over CPU time there):

| Spinners, QoS | Children | `plain` | `reused` | Clock |
| :--- | ---: | :--- | :--- | :--- |
| 0 | 24 | 3.282–3.591 ms | 1.744–1.972 ms | 4.02–4.24 GHz |
| 1, default | 11 | 2.849–2.966 ms | 1.913–2.004 ms | 4.15–4.23 GHz |
| 2, default | 8 | 2.841–2.951 ms | | 4.25 GHz |
| 3, default | 11 | 3.496–3.656 ms | 2.279–2.302 ms | 4.25 GHz |
| 1 or 2, background | 6 | 3.410–3.682 ms | 1.833–1.947 ms | 3.95–4.21 GHz |

The baseline is 3.398 ms (2026-11-01). One thread of other work at the default QoS, on a
performance core for the whole child, makes the item 17 % faster; three make it slower,
about 3 % in the middle of each range; threads at background QoS, which run on the
efficiency cores, leave it as it is. The one-minute load average, 1.58 to 3.00
throughout, does not tell these apart.

### Where the time goes

`phased`, each call's three steps in the fastest run, two children each:

| Spinners | `Field::new` | Update | Drop |
| ---: | :--- | :--- | :--- |
| 0 | 33.0–40.5 µs | 69.6–71.6 µs | 0.1–0.2 µs |
| 1 | 27.4–27.6 µs | 62.5–63.8 µs | 0.1 µs |
| 3 | 34.2–34.3 µs | 79.6–81.9 µs | 0.2 µs |

About a third of each call is `Field::new` on the calling thread alone: a zeroed vector of
5 337 088 bytes. After the first call it takes no page fault (0 in every later run), its
memory reused from the call before, so the step is zeroing in user space. `reused`, which
makes no field, takes 1.74–1.97 ms at rest; the update then reads and writes its 10.7 MB
on the four workers.

### The serial step alone

`zero`, 2000 vectors of the field's size made and dropped, in one process:

| Busy threads beside it | Fastest | Mean, back to back | Mean, 70 µs apart |
| ---: | :--- | :--- | :--- |
| 0 | 32.0 µs | 56.7–57.6 µs | 42.5–45.7 µs |
| 1 | 31.5–32.0 µs | 32.6–32.7 µs | 32.4–32.6 µs |
| 2 | 32.0 µs | 32.4–33.4 µs | 33.2–33.7 µs |
| 3 | 32.0–32.2 µs | 33.1–37.2 µs | 55.0–64.5 µs |

Every count reaches the same fastest time, and no vector faults. Alone, one thread zeroing
5.3 MB in bursts runs at that speed only part of the time, its mean 33 to 80 % slower;
with one or two other threads busy on the performance cores it keeps it. What in the chip
decides this is not seen by these counters: the cores' clock moves by at most 6 % (4.02
against 4.25 GHz), too little for the step's 30 to 80 %, and what is left, the cluster's
caches and memory path following its load, is not measured.

### At rest alone

Ten children with no spinner, one after another: `plain` 3.496–3.591 ms (2.7 %), all
3 to 6 % above the baseline; `Field::new` 34.2–37.4 µs a call in the fastest `phased` run.

## Findings

1. The item's time follows the performance cluster's state, which other processes' work on
   the performance cores sets, in either direction: one busy thread of other work makes it
   17 % faster, three about 3 % slower, work on the efficiency cores neither. A child
   lasts under half a second, so one state holds through all its rounds and both children
   of a pair, and the fastest of 20 rounds cannot leave it behind: a session's drift, not a
   pair's.
2. The step that moves most is `Field::new`, a third of each call, zeroing 5.3 MB on one
   thread: alone it runs at its fastest only part of the time (mean 33 to 80 % slower),
   with other threads busy on the cluster always. The parallel update moves too, 63 to
   82 µs a call.
3. The load averages the harness reads cannot see this: every child here started within the
   harness's rest, and one thread of other work is under one of load. What a session's
   baseline was recorded beside is not known; the baseline of 2026-11-01 sits between this
   session's quiet children (3.50–3.59 ms) and those beside one busy thread (2.85–2.97 ms).
4. No change to the item removes it: `reused`, the update alone, still moves with the
   cluster's load (1.74–1.97 ms at rest, 2.28–2.30 beside three threads).
