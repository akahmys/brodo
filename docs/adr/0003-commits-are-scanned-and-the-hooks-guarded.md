# ADR-0003: Commits are scanned for secrets, personal data and outside paths, and the hooks are guarded

- **Status**: Accepted
- **Date**: 2026-10-16

## Context

A secret, the owner's name or an absolute path into a home directory, once committed,
stays in the history. Commit hooks are skipped with one flag, and an agent skips them
readily when they fail.

## Decision

- betterleaks scans the staged change (`pre-commit`), the message (`commit-msg`), and in
  the audit the working tree and the whole history. Its default rules are extended, not
  replaced, and the audit fails when fewer than 100 load.
- The owner's name is matched by rules kept outside the repository, since a committed
  rule naming a person leaks the name.
- A relative path is resolved against the file holding it and fails when it leads outside
  the project (`scripts/audit/paths.py`).
- Everything fails closed: a missing scanner or rule file stops the commit.
- The audit fails while the hooks are not installed, and a Claude Code hook blocks
  `--no-verify`, a change to `core.hooksPath`, and `git push`.

## Consequences

A command written to evade the Claude Code hook can pass it; the history scan in the
audit is the backstop.
