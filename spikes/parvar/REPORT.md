# Spike: parvar (B-024)

Measured on 2026-10-31 with rustc 1.98.1, release build, on an Apple M5 (Mac17,2; 10
cores: cpu0–5 efficiency, cpu6–9 performance, as the IORegistry's `cluster-type` names
them). Every child started at a one-minute load below 2.5 and a five-minute load below
4.0. Reproduce from the repository's root:

```bash
cargo build --release --manifest-path spikes/parvar/Cargo.toml
spikes/parvar/target/release/parvar-spike 6
RAYON_NUM_THREADS=4 spikes/parvar/target/release/parvar-spike 6
RAYON_NUM_THREADS=1 PARVAR_KINDS=slab spikes/parvar/target/release/parvar-spike aslr 10
RAYON_NUM_THREADS=10 PARVAR_KINDS=wide,slab spikes/parvar/target/release/parvar-spike child out/spike/parvar/wide.tsv
```

The parent prints each child's fastest run of each kind; the rows of every run are kept
under `out/spike/parvar/`.

## Question

**B-024**: why do the leaf-parallel update and colino's `convert_slab_320` vary from one
benchmark process to the next, so that ADR-0019's 10 % refuses one pair in several?

## The harness

`src/main.rs` runs pairs of child processes, as the benchmark harness does. A child runs
its kinds in turn for 20 rounds, each after a 256 MiB eviction:

- `plain`: `leaf_parallel_update_x32` as the benchmark has it;
- `traced`: the same, with each leaf's rayon thread, CPU (`pthread_cpu_number_np`) and
  start and end (`mach_absolute_time`) recorded;
- `slab`: `convert_slab_320`;
- `wide`, `alu`: controls that touch no memory, 4096 parallel pieces of sixteen
  multiply-add and eight xorshift chains (`wide`) or four xorshift chains (`alu`).

Around each run it reads `proc_pid_rusage` (`rusage_info_v6`): CPU time and the part on the
performance cores, cycles and instructions and the parts on the performance cores, and
the time the process's threads were runnable. "Wait" below is runnable time less CPU
time: time a thread was ready but had no CPU.

Before it times anything a child asserts that the IORegistry names as many efficiency CPUs
as `hw.perflevel1.logicalcpu`, that a 200 ms spin reads as 150–300 ms of rusage time (the
unit is the 41.667 ns tick), and that a thread's samples of `pthread_cpu_number_np` fall
on the performance CPUs as often as rusage counts its time there. Both QoS probes ran on
the performance cores, so the efficiency numbering is checked only indirectly: leaves
traced on cpu0–5 took 3.5 times as long as those on cpu6–9 (about 180 against 52 ns), and
ran at 2.96 GHz against 4.25. The ASLR comparison asserts that its children without ASLR
share one image layout and those with it do not.

## Measurements

### The leaf-parallel update

Ten threads, 20 children (10 pairs), a one-minute load of 2.3–2.5 at each start:

| | Fastest | Pair | All |
| :--- | :--- | ---: | ---: |
| `plain` | 2.742–5.469 ms | 56 % | 99 % |
| `traced` | 3.036–7.167 ms | 83 % | 136 % |

Over the 20 children, the fastest traced run's length went with the wait (correlation
+0.96), with fewer threads taking a leaf (−0.95: from 10 to 6.9 a call) and with the time
from the last leaf to the call's end (+1.00: from 17 to 134 µs); not with the share of
leaves on the efficiency cores (−0.45). A call takes 60–90 µs; when another process's
thread takes a core, one worker waits, and the call waits for it.

Twelve children (6 pairs) at a one-minute load of 1.3–2.4: `plain` 2.727–2.870 ms, pairs
within 3.7 %; the wait was small.

### `convert_slab_320`

Instructions were the same in every run (22.31–22.49 G over 400 runs) and the clocks
steady (performance 4.25 GHz, efficiency 2.96 GHz, at 4 and 10 threads); the time moved
with the cycles, that is with the instructions per cycle (IPC), at −0.98 over all runs.

| Threads | Children | Fastest | Pair | IPC on the performance cores, all runs |
| ---: | ---: | :--- | ---: | :--- |
| 1 | 18 (ASLR, quiet pairs) | 782.5–832.7 ms | 1.7 % | 6.19–6.28 at the fastest |
| 4 | 12 (6 pairs) | 229.0–233.1 ms | 1.1 % | 6.17–6.23 at the fastest |
| 10 | 12 (6 pairs) | 157.9–211.9 ms | 32 % | 2.39–4.72 (all cores) |

- **Not the code's layout.** At one thread, 10 children without ASLR and 10 with it,
  alternating: on pairs 1–9 both gave 782–833 ms at IPC 6.2–6.3, each pair within 1.7 %.
  Pair 0 ran at a five-minute load of 3.93; its child with ASLR ran at IPC 5.8–5.9
  throughout (926 ms). The heap's places differ between children with ASLR off, so the
  heap's layout is not tested.
- **Not waiting for a CPU, nor idle spinning.** At ten threads the fastest runs waited
  3.7–52 ms of 1361–1918 ms of CPU time, with no relation to their time (+0.07 across
  the 12 children); system time was 3–11 ms a run; CPU time was about ten times the wall
  time, every thread busy throughout.
- **Not power.** The `wide` control, IPC 3.93 on one thread, kept 3.93 at 4 threads and on
  both core types at 10; its fastest at ten threads was 30.29 and 30.36 ms in two children.
  `alu` kept 1.17 at 1, 4 and 10 threads.
- **Within one process too.** At ten threads one child's 20 runs took 158–231 ms, its IPC
  3.06–4.72; the fastest of 20 is the luckiest run, and that is what differs between the
  children.
- **Both core types lose IPC together**, from 6.25 to 2.8–5.0 on the performance cores
  and to 2.0–2.7 on the efficiency cores, when the slab's value step runs on all ten
  threads; at four threads, all on the performance cores, it keeps 6.2.
- No state is shared between the value step's threads in colino's code: its one
  `thread_local!` is compiled in tests alone (`crates/colino/src/bvh.rs`).

What in the chip takes the cycles is not seen by these counters: the controls rule out
power and the scheduler; the slab's own traffic through the shared caches and memory
when both clusters run it is what is left, and is not measured.

## Findings

1. The leaf-parallel update varies with the load of other processes: at ten threads on
   ten cores, a call of under 100 µs waits for any worker another process displaces. At a
   one-minute load below about 2.4 its pairs agree within 3.7 %; the harness starts a
   pair at a load below 3.0 (ADR-0019).
2. `convert_slab_320` varies with the IPC of its value step when its threads run on both
   core types: 32 % between pairs at rest at ten threads, 1.1 % at four. Neither the
   scheduler, nor power, nor the code's layout, nor waiting for a CPU explains it.
3. At four threads, as many as the performance cores, both items' pairs agree within
   1.3 % (12 children); the slab takes 229 ms against 158–212 at ten.
4. One thread is no answer: the slab takes 783–833 ms, and the update's parallel path
   would not be measured.
