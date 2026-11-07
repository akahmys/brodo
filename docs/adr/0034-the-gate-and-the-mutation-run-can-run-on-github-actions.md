# ADR-0034: The gate and the mutation run can run on GitHub Actions

- **Status**: Amended
- **Date**: 2026-10-24

Condensed on 2026-11-08 (ADR-0001): each decision is kept with its reason and what it set
aside; the text before is in git, this file as of `53f69d5`.

## Context

P3's mutation run took about 45 hours on the owner's machine, two jobs at a time, and
82 of brodo's 96 timeouts came before any test failed, the machine's other work slowing
the suite past its timeout (2026-10-23). GitHub's documents (2026-10-24) gave: a job runs
six hours at most; standard runners are free for a public repository, a private one's
2,000 minutes a month then $0.006 a minute. cargo-mutants' `--shard k/n` runs one of n
parts of the mutants, leaving their reports to be taken together. Two of the gate's checks
read files kept only on the owner's machine: the local list of forbidden sources
(AGENTS.md S1) and the personal rules (AUDITING.md §2), which cannot be put on a remote
machine without leaking what they guard.

## Decision

The owner decided, on 2026-10-24 unless dated otherwise.

- **Two workflows.** `.github/workflows/audit.yml` runs the gate, and
  `.github/workflows/mutants.yml` the mutation run in shards, a last part taking their
  reports together. On GitHub Actions the gate skips the two checks that read the owner's
  local files and says so; the gate run on the owner's machine before every push still
  runs them, so no check is lost (shown, 2026-10-25, by the gate with `GITHUB_ACTIONS=true`
  passing and failing at exactly those two without it).
- **When they run.** `audit.yml` runs on each push to `main` and by hand, from
  2026-10-25, when the repository went public and standard runners became free; it passes
  the gate the commits the push brought, `COMMIT_RANGE`, held to `commit_msg_check.py`
  (2026-11-02). `mutants.yml` runs by hand only. No pull request starts either, so no
  fork's code runs on the repository's runners. Before going public, the owner had the
  history rewritten by `git filter-repo --replace-text`, the names of the local list's
  projects and of one project's component leaving every commit, and force-pushed `main`
  and the tags.
- **Other platforms** (2026-11-04). `audit.yml`'s job `platforms` runs the gate's
  `cargo test --workspace --all-features` on `macos-15` (arm64) and `windows-2025`
  (x86-64), each to its end whatever the other gives: the `mmap` feature, paths and
  `mestolo`'s refusals had never run on Windows. The rest of the gate does not depend on
  the platform and runs on Linux alone, as do fuzzing, Miri, the mutation run and the
  benchmarks.
- **The mutation run's shape.** 160 shards by default, about 23 mutants each, within six
  hours (2026-10-27: at 80 two of colino's heaviest were cut off); the timeouts run again
  under 1800 s in a matrix of 20 parts, `--shard k/20`, since GitHub Free runs 20 jobs at
  once and one job running them all was cut off. A part lists the mutants it is dealt and
  fails unless it reports each timeout among them (2026-11-02); a deleted struct field,
  which `--re` lets through, is run but not judged. Further arguments reach `mutants.py`
  through `MUTANTS_ARGS`, split as a shell splits words with nothing expanded, and each
  shard keeps `rss_guard.log` beside its report (2026-11-02: unquoted, a glob was
  expanded by the shell). `mutants.py --report` and `--patient --from` take the shards'
  reports (ADR-0023).
- **What runs is pinned** (2026-10-24, after a review by Anthropic's article on
  attack-surface reduction found two ways in for someone controlling a dependency of the
  build). Each action is pinned to a release's commit, the release named beside it, not a
  movable tag. betterleaks is fetched from its release and checked against the SHA-256
  GitHub states for the asset.
- **One version of each tool** (2026-11-07). The gate fails where the betterleaks that runs
  is not the version `audit.yml`'s URL names, or the running `cargo-deny`,
  `cargo-mutants` or `cargo-semver-checks` not the version `audit.yml`'s `cargo install`
  line names, or where a workflow installs another version of one: those lines are each
  version's one home. The gate on GitHub had failed ten times in two days, unread, after
  Homebrew raised the owner's betterleaks to 1.10.0 while `audit.yml` installed 1.9.0,
  which read the configuration file unasked.
- **The run read** (2026-11-07). The last push's run, every job, is read before the next
  commit lands, and a failure fixed first (TESTING.md §4), held by nothing since the gate
  needs no GitHub access.
- **The workflows are rules**: AGENTS.md lists `.github/workflows/`, and the `commit-msg`
  hook asks an ADR of a commit touching them.

## Consequences

- A private repository's 2-core runner took about 5.3 minutes a mutant against about 2 on
  the owner's machine (2026-10-25); public, the runs cost nothing, and a whole run takes
  about its work over 20 jobs.
- What the gate checks on Linux alone is assumed not to depend on the platform.
