# ADR-0061: brodo, colino and mestolo are released as 1.0.0

- **Status**: Accepted
- **Date**: 2026-11-08

## Context

ADR-0011 left 1.0 to an ADR of its own when P9 closed (TESTING.md V5). By 2026-11-08 every
item of P9 but the release itself was done (ROADMAP.md P9): the formats settled before
any stability is promised (ADR-0053, ADR-0054), the public API reviewed and the designs
grown by accretion taken (ADR-0054 to ADR-0058), T14's gaps taken (ADR-0059, ADR-0060),
the rules for versions after 1.0 and the check of a release against the last one
(ADR-0011, amended), the packages (ADR-0040, amended), and the closing checks: a fuzz
round and a Miri run with no finding, a mutation run whose misses were killed or excluded
(ADR-0023, amended), and the benchmarks, `leaf_parallel_update_x32` left slower than its
baseline by the owner (ADR-0019, amended).

## Decision

The owner decided, on 2026-11-08, that the next release is 1.0.0: the workspace version
is set to `1.0.0`, and the commit that sets it is tagged `v1.0.0` and released as TESTING.md
V4 says, the three crates then published to crates.io by the owner, brodo, then colino,
then mestolo.

## Consequences

- From 1.0.0 the promises of AGENTS.md's three homes and of `mestolo`'s command line and
  output follow semantic versioning (V2), and `scripts/audit/semver.py` compares each
  release with the newest release tag from `v1.0.0` on (V9).
- V6's exception before 1.0 (ADR-0053) no longer applies: a release raising a format
  version reads the version before it.
- Users depend on the crates by version from crates.io (ARCHITECTURE.md §7), and README.md
  says so.
