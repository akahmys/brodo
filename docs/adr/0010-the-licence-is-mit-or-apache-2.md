# ADR-0010: brodo is licensed MIT OR Apache-2.0

- **Status**: Accepted
- **Date**: 2026-10-16

## Context

brodo is meant to be published on its own and used by projects under other licences,
copyleft ones among them.

## Decision

- Both crates are MIT OR Apache-2.0, stated once in `[workspace.package]` and inherited
  (`license.workspace = true`, checked by `scripts/audit/workspace.py`).
- `LICENSE-MIT` and `LICENSE-APACHE` are at the root; the MIT notice names `akahmys` as
  the copyright holder. The Apache text was copied from a crate in the local cargo
  registry rather than written from memory.

## Consequences

Dependencies stay permissive (ADR-0005), and copyleft sources stay unread (ADR-0007).
