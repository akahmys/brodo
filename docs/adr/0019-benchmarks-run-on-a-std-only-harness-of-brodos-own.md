# ADR-0019: Benchmarks run on a std-only harness of brodo's own, and fail 10 % behind their baseline

- **Status**: Amended
- **Date**: 2026-10-17

## Context

TESTING.md T5 asks a performance comparison for the fastest of many runs of each variant,
the variants run in turn, each run from evicted caches, each comparison in a fresh
process, and the whole run twice with its agreement stated. B-007 asked which harness
does this. The human limited the spike `spikes/bench/` to a harness of brodo's own,
measured alone; criterion and divan were not tried. On an Apple M5 on 2026-10-17
(`spikes/bench/REPORT.md`), about 150 lines of std met every clause of T5. At 20 rounds
with a 256 MiB eviction buffer, the two fresh processes of a pair agreed within 1.7 %;
5 rounds disagreed by up to 4.6 %, and 32 MiB left most of the working set cached.
Absolute times drifted by up to 7.9 % within one session of about 40 minutes.

## Decision

- The benchmarks are `harness = false` targets in `crates/brodo/benches/`, on std only,
  with no dependency added. The runner that compares them with their baselines is part of
  the same benches, not a separate tool.
- Each comparison runs in pairs of fresh processes; each process runs every variant once
  per round, in turn, for 20 rounds, writing over a 256 MiB buffer before each run, and
  keeps each variant's fastest time.
- A benchmark fails its baseline when it is more than 10 % slower. The human chose 10 %
  above the drift measured in one session (7.9 %).

## Consequences

The drift between days, between the performance and efficiency cores, and with the
machine's temperature was not measured, so 10 % may fail a change that made nothing
slower; such a failure is a reason to measure the drift again and amend this record. A
slowdown under 10 % passes unseen by a baseline comparison; comparing variants in turn in one process remains the way to
see one. The eviction size was measured on one machine only.

## Amendment

- **Date**: 2026-10-17

The first baseline was recorded while other work ran on the machine. Compared with it,
with no change to the code, the leaf-parallel update failed twice in a row at +21 to
+27 % in both children, then passed four times. With the other work stopped, six
comparisons passed, but items whose run took under 3 ms (the cache map at 0.06 ms, the
parallel update at 0.2 ms) disagreed within a pair by up to 14 %. So:

- An item that runs in under 3 ms is repeated until a run lasts 3 ms or more, and the
  count is part of its name (`cache_map_x256`), so that a baseline taken at another count
  is not compared.
- Benchmarks are run and recorded on a machine doing nothing else; the benches say so.

Recorded again at rest, every item took 3.0 to 5.5 ms per run. All six comparisons that
followed passed; pairs agreed within 3.4 % except the parallel update, within 10.7 % (its
ten threads share four performance and six efficiency cores), whose change from its
baseline ranged from −5.5 to +11.3 %, the other items' from −1 to +6 %. The parallel update therefore sits near the tolerance and may fail without
cause; it passed each time because a failure needs both children slower.

## Amendment

- **Date**: 2026-10-17

P1's retrospective asked for a check against a baseline recorded on a busy machine. A
refusal by the spread within a pair would not have caught the first one: while other work
ran, the parallel update's two children agreed within 3.2 % and 1.2 %, slow alike. The
one-minute load average was read instead, on the Apple M5: 1.53 to 2.13 over five idle
minutes, about 2.0 before and during benchmarks run back to back, 3.86 just after a build
and the tests. So a run, comparison or `--record`, first reads it and, while it is above
3.0, waits ten seconds and reads again, up to two minutes, then refuses. The load during
the other work of the first incident was not measured, so whether 3.0 would have caught
it is not known; nor was any machine but the M5 measured. A threshold of 0.5 refused a
run after one wait.

## Amendment

- **Date**: 2026-10-17 (when benchmarks run)

The benchmarks ran only when someone chose to. A refactoring that moved the leaf index into
another module made gather 50 % slower, caught only because the benchmarks were run by
hand. The audit review asked for a rule: TESTING.md T10, a change to `src/` on a path a
benchmark times is benchmarked before it lands and does not land while an item fails. It
is held by review: nothing checks that a commit was benchmarked.


## Amendment

- **Date**: 2026-10-20

colino's conversion needed watching after it was measured at scale: at a sphere radius
of 400 voxels it took 33 s and labelling 9.5 s, at 200 voxels 6.9 s and 2.4 s, the
winding number 77 % of the conversion's values (ADR-0027). The human chose to measure
before changing it, and to share the harness rather than write a second (PLANNING.md §2).

- The harness moved, unchanged in what it does, to `crates/brodo/benches/harness/mod.rs`;
  each member's benchmark includes it, colino's by `#[path]`, and calls `run` with its
  data and items. Each writes to `out/bench/<crate>/`, so that two members' children do
  not overwrite each other's files.
- colino's items: a sphere of radius 50 voxels, 8192 triangles, converted with a band of 3
  (325 ms), and labelled (165 ms), recorded on 2026-10-20 at rest.
- brodo's items gave the same verdicts through the shared harness: all within their
  baselines at rest, the leaf-parallel update at +2.7 and +9.9 %. A first run, with
  another program using more than one core, failed that item at +16 % in both children
  though the load average let it start: one more reading for the open question on the
  load average (ROADMAP.md §1).

Including a file outside the crate means colino's benchmark cannot be packaged as it
stands; before colino is published, its benchmark is excluded or the harness becomes a
crate of its own.

## Amendment

- **Date**: 2026-10-20

`gather_w3` was added after the baselines of 2026-10-17. `--record` replaces every row of
the hardware model, so recording the new item would have reset the rest, against which
drift is measured; its row was added by hand instead. P2's retrospective proposed, and
the human decided:

- `--record NAME` replaces or adds that item's row alone, with the faster of the pair, the
  other rows kept with their dates; a name no item has is refused. `--record` alone
  records every item, as before.

Shown, at a load average of 2.5 to 2.6: with the name ignored, `--record gather_w3`
rewrote all ten rows; `--record nosuch` was refused and changed nothing; `--record
gather_w3` changed its row alone, 4437000 to 4358416 ns, the other nine keeping
2026-10-17.

## Amendment: the load a run starts at

- **Date**: 2026-10-25

P4's retrospective found that whether a run was at rest was measured, the harness waiting
while the one-minute load average was above `BUSY_LOAD`, but not kept. The owner decided
that the report keeps it: `report.tsv`'s header names the load average the run started at,
beside the hardware and the date, recorded only, the run still refused above
`BUSY_LOAD`. Shown: colino's benchmark at rest wrote `load 2.93` in its header.


## Amendment: the five-minute load

- **Date**: 2026-10-26

P5's retrospective found runs that started at rest by the one-minute load average while the
five-minute stayed high, after a build, when the system's own scans of the new binaries
held the machine busy for minutes: at 2.48 and 10.12 a pair disagreed by 16.9 %; at 2.43
and 3.46, Hermite data's items read 8 to 13 % slow, which back-to-back runs at 2.34 and 4.40,
and 1.93 and 3.92, put down to the machine; P5's closing runs, at 1.72 and 3.83 and at 1.50
and 3.16, agreed within 7.8 %. The owner decided that the harness waits while the
five-minute load average is above 4.0 as well, `SETTLED_LOAD`, for up to ten minutes rather
than two, the report's header naming both loads, and that its documentation says dates are
UTC. A five-minute load of 4.0 would not have held back the run at 3.46. Shown to fire:
with `SETTLED_LOAD` at 0.5, a run at loads of 2.51 and 4.13 was refused, naming both.

## Amendment: two baselines moved at P5's close

- **Date**: 2026-10-28

P5's closing run failed two of brodo's items in both runs, `cache_write_x8` at +689 and
+678 % and `gather` at +15.0 and +14.9 %, against baselines of 2026-10-17. Benchmarked
commit by commit from `9940bd5`, the last run within them, at five-minute loads of 3.2 to
4.0, each moved at one commit, none of which recorded a benchmark as T10 asks:

- `cache_write_x8`, 5.2 to 43 ms at `e25e1a9`: `cache::write` syncs the hidden file to the
  disk before renaming it over the path, as the owner chose for review G1, `sync_all`
  being `F_FULLFSYNC` on macOS, about 4.6 ms a file. Unsynced, a crash could leave the
  renamed file without its bytes in place of a good one.
- `gather`, 3.09 to 3.58 ms at `68543d9`, which touched no code on its path: `gather` and
  `sources` disassembled, at the two commits, to the same 287 lines, only their addresses
  differing, the first moving from 4 to 52 bytes past a 64-byte boundary. A change in code
  placement, not in the code.

The owner decided that the sync stays and both rows are recorded again, `--record
cache_write_x8` and `--record gather`, at loads of 1.44 and 1.41: 43 486 750 and
3 468 083 ns. A run then put every item within its baseline, the two at +0.8 and −2.0 %,
and −0.6 and −0.2 %. Code placement alone moves an item by as much as 15 %, which the
10 % tolerance cannot tell from a change in the code; it is read by disassembling, as here.

## Amendment: a pair that disagrees is refused

- **Date**: 2026-10-30

P6's retrospective found a spike's run that started at rest, at a one-minute load below
2.5 and a five-minute below 4.0, and read 1.3 to 2.5 times slower than two later runs,
which agreed within 15 % (spikes/region/REPORT.md). A load read at the start cannot see
work that begins during a run, and one read at the end cannot tell it from the run's own:
ten busy threads, as many as the machine's processors, took the one-minute load from 2.40
to 9.02 in 90 s, above `BUSY_LOAD` with nothing else running. The owner decided that the
harness refuses a pair whose two children disagree on an item by more than 10 %,
`SPREAD`, comparing or recording, and judges and records nothing from it, the report still
written: at rest pairs agreed within 7.8 % (P5's close) and 4.2 % at most in the last
reports of 2026-10-28, and a pair disturbed by other work disagreed by 16.9 %. The
refusal is checked by the harness; a spike's timings, which do not run on it, are held by
review alone. Shown to fire: with `SPREAD` at 0, colino's run at loads of 1.82 and 1.50
was refused, naming every item.

At 10 %, at rest, colino's `convert_slab_320` alone was refused, at 17.2 % and 20.5 % in
two runs of three, while its other items agreed within 3.8 %. Ten children run one after
another put its fastest time at 154 to 191 ms, a different one each process, the other
items within 6 %; on 4 threads at 227 to 232 ms. Timed by step, the leaf-parallel values
step held all of it, 156.8 to 179.5 ms, the serial steps within 0.6 ms; no other process
used 5 % of a processor for more than one 2-s sample in a child. The owner decided that the
limit stays at 10 %, a refused pair run again, and the item's variation is an open question
(ROADMAP.md B-024).

## Amendment: every feature

- **Date**: 2026-10-30

P6's closing checks found no benchmark item for the archive, whose feature is not a
default one, so that an item behind it would be left out, unseen, by T10's command. The
owner decided that brodo's benchmark gains `archive_write`, `archive_read`,
`archive_read_8bit` and `archive_region`, the last refused before it is timed if its box
holds no leaf (PLANNING.md §3), and that every benchmark runs with every feature, as the
gate's tests do: `cargo bench -p <crate> --bench <crate> --all-features` (TESTING.md T10).

P6's closing benchmarks, at rest, at one-minute loads of 1.81 to 2.03: two pairs were
refused, `leaf_parallel_update_x32` at 12.5 and 10.3 % (ROADMAP.md B-024's question), and
the third put every item within its baseline. Children of `6013b72`, before P6's
refactoring, and of `644fe47`, after it, run in turn three times each:

| Item | `6013b72` | `644fe47` | Baseline |
| :--- | ---: | ---: | ---: |
| `cache_read_x16` | 2.76 to 2.89 ms | 2.78 to 2.86 ms | 3.98 ms, 2026-10-17 |
| `cache_map_x256` | 4.35 to 4.53 ms | 3.97 to 4.09 ms | 3.86 ms, 2026-10-17 |
| `gather` | 3.46 to 3.54 ms | 3.02 to 3.09 ms | 3.47 ms, 2026-10-28 |

`cache_read_x16` had grown faster before the refactoring, its baseline left 28 % slow;
`gather`, whose code the refactoring did not touch, moved by 13 % between the two builds,
as code placement moved it at P5's close; `cache_map_x256`, whose path the refactoring
touched, by 9 %, its cause not told apart. `cache_write_x8`'s slow first child did not
recur in four pairs, within 2.6 %. The owner decided that the archive's four items and
`cache_read_x16` are recorded, one `--record NAME` each, at one-minute loads of 1.39 to
1.97: `archive_write` 5 140 417, `archive_read_x8` 3 948 083, `archive_read_8bit_x8`
3 167 167, `archive_region_x8` 3 471 458 and `cache_read_x16` 2 850 416 ns; that
`gather` and `cache_map_x256` keep theirs, which a build's placement moves both ways; and
that the two cache items leave ROADMAP.md's unresolved problems with these figures.
