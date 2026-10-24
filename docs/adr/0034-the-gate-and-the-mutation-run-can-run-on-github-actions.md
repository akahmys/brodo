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
