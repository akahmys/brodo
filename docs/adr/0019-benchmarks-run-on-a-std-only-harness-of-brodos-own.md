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

