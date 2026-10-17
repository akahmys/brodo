# ADR-0022: Every test is listed, so that a deleted test needs an ADR

- **Status**: Accepted
- **Date**: 2026-10-17

## Context

TESTING.md T2 forbade deleting or disabling a test without an ADR, held by review alone.
The audit review before P1 closed found that the gate reported a count of tests passed and
nothing more: a test deleted, renamed or compiled out lowered the count and passed.

## Decision

- Each member keeps `test-list.txt` beside its `Cargo.toml`: one line per test of its
  library, binaries, integration tests and examples, ignored ones included, as each test
  executable lists them (`--list`). `scripts/audit/tests.py` builds the executables, lists
  their tests and fails when a test is missing from the list or a listed test is gone;
  `--update` rewrites the lists.
- `commit_msg_check.py` treats a line removed from a `test-list.txt` as a rule change: the
  message names an ADR and an ADR is staged.

Shown to fire: renaming one test failed `tests.py`; in a scratch clone, a commit removing a
line from the list was refused without an ADR and accepted with one, and a commit adding
a line passed.

## Consequences

A renamed test needs an ADR, as a deleted one does. Adding a test takes one more command.
Benchmarks and doc tests are not listed: brodo has no doc tests, and benchmarks are not
tests. A loosened tolerance is still held by review alone.
