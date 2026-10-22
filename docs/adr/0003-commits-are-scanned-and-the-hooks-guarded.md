# ADR-0003: Commits are scanned for secrets, personal data and outside paths, and the hooks are guarded

- **Status**: Amended
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

## Amendment (2026-10-17): `out/`

CODING.md L2 already said that output and tool environments go under `target/` or `out/`
and that the audit's scans skip both, but `.betterleaks.toml` skipped only `target/`. A
Python virtual environment under `out/`, which holds absolute paths into the home
directory, failed the gate, and was moved to `target/` as a workaround. `out/` was added
to the file's path allow-list, so the secret scan skips the same directories as
`scripts/audit/documents.py` and `scripts/audit/paths.py`. The pattern matches a
directory named `out/` at any depth, as the one for `target/` does, so a file committed
under such a directory would be skipped too; nothing checks that none is. Measured with a
file holding an absolute path into a home directory: before the change the working-tree scan found it under
`out/t3/` and `t3probe/`; after, under `t3probe/` and `t3probe/layout/` only.

## Amendment (2026-10-17): one list of output directories

P1's retrospective found that the mismatch above had stood unnoticed because each of the
three places, `.gitignore`, `.betterleaks.toml` and the scans' skip lists, named the
directories on its own. They were listed once, as `OUTPUT_DIRS` in
`scripts/audit/project.py`, which `paths.py` and `documents.py` skip, and `paths.py` fails
when one of them is missing from `.gitignore` or from `.betterleaks.toml`'s allow-list.
Removing `out/` from the allow-list, and `/target/` from `.gitignore`, each failed it.

## Amendment (2026-10-22): pushing `main` and version tags

The guard blocked every `git push` until the human decided the repository had a remote. It
has one, `origin`, and the owner decided that an agent pushes what the gate has passed:
`git push origin` with `main` and tags of the form `vX.Y.Z` (TESTING.md V4), and nothing
else. A push with any option (`--force`, `--delete`, `--mirror`, `--all`, `--tags`), a
`+` forcing a ref, a `:` naming another, another remote or another ref stays blocked, as
does a bare `git push`. Shown to fire: each blocked form was refused by the hook and each
allowed one passed, by feeding it the tool call's JSON.

## Amendment (2026-10-22): redirections

P3's retrospective found `git push origin main 2>&1` blocked though its push was one the
guard allows: the guard split commands at `&`, so `2>` became a ref. The human decided
that the guard drops a redirection and its target, `2>&1`, `> out.log` or `&> all.log`,
before it splits a command. Shown to fire: of three pushable commands with redirections,
the guard before the change blocked two and after it blocked none; a forced push, another
ref, a bare push and `--no-verify` stayed blocked with a redirection added.

