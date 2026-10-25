# ADR-0034: The gate and the mutation run can run on GitHub Actions

- **Status**: Accepted
- **Date**: 2026-10-24

## Context

P3's mutation run took about 45 hours on the owner's machine, two jobs at a time, and
82 of brodo's 96 timeouts came before any test failed, the machine's other work slowing
the suite past its timeout (2026-10-23). Milestones had closed about every two days.
GitHub's documents (2026-10-24) gave: a job runs six hours at most; a private
repository's standard Linux runner has 2 cores and 8 GB, a public one's 4 and 16 GB;
2,000 free minutes a month on GitHub Free, then $0.006 a minute for 2-core Linux x64;
standard runners free for public repositories. cargo-mutants' `--shard k/n` runs one of n
parts of the mutants, each part finding them itself, and leaves taking the parts together
to the caller. Two of the gate's checks read files kept only on the owner's machine: the
local list of forbidden sources (AGENTS.md S1) and the personal rules (AUDITING.md §2),
which cannot be put on a remote machine without leaking what they guard.

## Decision

The owner decided, on 2026-10-24, to prepare remote runs while the repository stays
private and its publication is decided later:

- `.github/workflows/audit.yml` runs the gate, and `.github/workflows/mutants.yml` the
  mutation run in n shards with a last job taking their reports together and, when asked,
  running the patient timeouts. Both start by hand only, so that they spend nothing until
  asked and run for no pull request from a fork.
- On GitHub Actions the gate skips the two checks that read the owner's local files and
  prints that it did; the gate run on the owner's machine before every push still runs
  them, so no check is lost.
- `scripts/audit/mutants.py --report` and `--patient --from` take several reports, the
  shards of one run.
- Tools are installed at the versions the owner's machine runs.
- The workflows are rules: AGENTS.md lists `.github/workflows/` among them, and the
  `commit-msg` hook asks an ADR of a commit touching them. Shown to fire: a commit of
  `audit.yml` with no ADR named was refused, naming the file.
- Shown to work on the owner's machine: `--report` over P3's brodo report and P2's colino
  report gave 115 failures, their 99 and 16, and over brodo's report twice gave 99.

## Consequences

Whether the workflows work is shown only by a first run, which spends minutes of the free
allowance; it is held until the local mutation run no longer needs the machine. Moving a
milestone's mutation run there, and how often, is decided with P4 and ADR-0033.

## Amendment

- **Date**: 2026-10-24

Anthropic's article on attack-surface reduction (2026) argued for shrinking what an
attacker can reach rather than chasing each bug, and a review of the workflows by it found
two ways in for someone controlling a dependency of the build: actions named by a movable
release tag, and betterleaks installed at whatever version Homebrew held. The owner
decided:

- Each action is pinned to the commit of a release, the release named beside it:
  `actions/checkout` v7.0.1, `actions/upload-artifact` v7.0.2,
  `actions/download-artifact` v8.0.2, looked up through GitHub's API on 2026-10-24.
- betterleaks 1.9.0, as on the owner's machine, is fetched from its release and checked
  against the SHA-256 published in its `checksums.txt` before it is installed.
- The rest of the review, which touches the public API, goes to P4.

## Amendment

- **Date**: 2026-10-25

The gate's GitHub mode was shown to work on the owner's machine before any run on GitHub:
with `GITHUB_ACTIONS=true` and the two local files pointed at nothing, the gate passed,
saying it skipped the list and the personal rules; without it, the same run failed at
exactly those two checks.

## Amendment

- **Date**: 2026-10-25

The first runs on GitHub held. `audit.yml` passed in 27 minutes, saying it skipped the two
local checks, 211 tests passing. `mutants.yml`, over `mesh.rs` in two shards, caught all
19 mutants in 57 minutes, the memory guard logging on Linux, and its last job took the two
reports together and passed. A private repository's 2-core runner took about 5.3 minutes a
mutant, against about 2 on the owner's machine at two jobs: a whole run of some 2950
mutants would take about 260 runner hours, 15 600 minutes, some $82 beyond the free 2000
at $0.006 a minute, and at least 45 shards to keep each within six hours, about 13 hours
at 20 jobs at once. Where a milestone's run goes is decided when it closes.

## Amendment: the gate on every push

- **Date**: 2026-10-25

Before making the repository public, the owner had its history rewritten: the names of the
projects in the local list of forbidden sources, and of one project's component, left
every commit by `git filter-repo --replace-text`, the table kept outside the repository;
`HEAD`'s tree and those of `v0.3.0` and `v0.4.0` stayed the same; the owner force-pushed
`main` and the tags, and the two workflow runs on the old commits were deleted. Standard
runners being free for a public repository, the owner then decided that `audit.yml` runs
on each push to `main` as well as by hand; no pull request starts it, so no fork's code
runs on the repository's runners. `mutants.yml` still starts by hand only.

