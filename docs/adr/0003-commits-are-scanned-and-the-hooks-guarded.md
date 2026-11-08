# ADR-0003: Commits are scanned for secrets, personal data and outside paths, and the hooks are guarded

- **Status**: Amended
- **Date**: 2026-10-16

Condensed on 2026-11-08 (ADR-0001): each decision is kept with its reason and what it set
aside; the text before is in git, this file as of `53f69d5`.

## Context

A secret, the owner's name or an absolute path into a home directory, once committed,
stays in the history. Commit hooks are skipped with one flag, and an agent skips them
readily when they fail.

## Decision

The owner decided, on 2026-10-16 unless dated otherwise.

- **What is scanned.** betterleaks scans the staged change (`pre-commit`), the message
  (`commit-msg`), and in the audit the working tree and the whole history. Every text file
  git would commit, staged or in the history, and every SVG, is also passed to
  `betterleaks stdin` by `leak_input.py`, where no path is matched (2026-11-02):
  betterleaks' default allow-list skips `.svg`, `.gltf`, `node_modules/` and more by name.
- **The rules.** The project's extend betterleaks' default, never replace them. Each call
  names `-c .betterleaks.toml`, and the count passes only when more rules load from the
  file than the default holds (2026-11-05: betterleaks 1.10.0 reads the file only when
  given it, and four calls of the gate had run on the default rules alone, which also met
  the former bound of 100). The owner's name is matched by rules kept outside the
  repository, since a committed rule naming a person leaks the name. Commit messages are
  held to the project's rules too (2026-11-02).
- **Paths.** A relative path is resolved against the file holding it and fails when it
  leads outside the project (`scripts/audit/paths.py`). The output directories are listed
  once, `OUTPUT_DIRS` in `scripts/audit/project.py`, and `paths.py` fails when one is
  missing from `.gitignore` or from `.betterleaks.toml`'s allow-list, which names each
  where `.gitignore` ignores it, at the root alone (2026-10-17, 2026-11-02: three lists
  had drifted apart, and `out/` at any depth let `crates/x/out/` be committed unscanned).
  `paths.py` and `documents.py` read the files git would commit, SVGs among them.
- **Everything fails closed**: a missing scanner or rule file stops the commit.
- **The guards are a convenience; the backstop is the audit** (2026-11-02). The audit fails
  while the hooks are not installed. A Claude Code hook, `claude_guard.py`, runs on every
  tool, splits words as the shell does, finds `git` wherever it stands in a command
  (quoted, as a path, after `env`, `sudo`, `command`, shell keywords and in subshells),
  and blocks `--no-verify` however abbreviated or quoted, a change to `core.hooksPath` but
  installing the hooks, git's configuration changed by the environment, an include or an
  alias, a remote's URL, and plumbing that runs no hook. A command written to evade it can
  pass; the audit holds each commit not yet pushed, and on GitHub each commit a push
  brought, to `commit_msg_check.py`, and the history scan holds every commit.
- **Pushing** (2026-10-22). An agent pushes what the gate has passed: `git push origin`
  with `main` or a tag of the form `vX.Y.Z` (TESTING.md V4), and nothing else; any option
  before or after `push` but `-C`, a `+` or `:` in a ref, another remote or ref, and a bare
  `git push` stay blocked. A redirection and its target are dropped before the command is
  split, so that `2>&1` is not read as a ref.
- **The commit message** (2026-11-02) is read up to `git commit -v`'s scissors line, and
  must name an ADR that is staged where a rule file changes; files are listed without
  rename detection, a lint level written as a table is read, and CJK punctuation is
  refused as not English.

## Consequences

Each guard's gaps found by review, 11 of 25 commands, then 10 more, then 13 of 63, were
closed one by one before the guard was made a convenience behind the audit; the audit's
checks of commits and history, not the guard, are what hold.
