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

## Amendment: what the guards read, and what the scans skip

- **Date**: 2026-11-02

The second review of the whole repository ran commands past `claude_guard.py`: `--no-verif`,
which git takes for `--no-verify`; `git -c 'core.hooksPath=…'`, its setting quoted;
`GIT_CONFIG_*` variables; a commit in a subshell, after `if`, or in `$(…)`; `git -c
remote.origin.url=… push origin main`; and an alias made with `git config alias.…`. It found
`.claude/settings.json` running the guard on Bash alone, so a terminal tool's command passed
it; `commit_msg_check.py` naming a renamed rule file by its new name only, missing a lint
level written as a table, and taking CJK punctuation for English; and betterleaks'
allow-list, `paths.py` and `documents.py` skipping `target/` and `out/` at any depth though
`.gitignore` ignores them at the root alone, so that `crates/x/out/` could be committed
unscanned. The owner decided, on 2026-11-02, that the guard reads quoted settings and
`GIT_CONFIG_*` variables, blocks an abbreviated `--no-verify`, an alias and any option before
`push` but `-C`, and finds a git command after shell keywords and in subshells; that it runs
on terminal tools too; that `commit_msg_check.py` lists files without rename detection,
reads a table's `level`, and refuses CJK punctuation, as `documents.py` does; and that the
allow-list names each directory where `.gitignore` ignores it, while `paths.py` and
`documents.py` read the files git would commit. Shown to fire: of 25 commands, the old
guard missed 11 it should block and the new one none, passing the 14 it should pass; the
three `commit_msg_check.py` cases passed the old check and failed the new; a Japanese line in
`crates/brodo/out/` passed the old `documents.py` and failed the new; betterleaks with the
new allow-list found a home path in `sub/out/` and skipped `out/` at the root.

## Amendment: commands written another way, and a message's scissors

- **Date**: 2026-11-02

An outside review of the uncommitted change ran ten more commands past the guard: `git config
--unset core.hooksPath scripts/hooks`, its value pattern read as the install; `"--no-verify"`
and `"-n"` quoted; `"git"`, `/usr/bin/git`, `env -i git` and `command -p git`; `-P` or
`--git-dir .git` before `push`, which the guard read as no push; and a command after a heredoc
operator on its line, removed with the body. It found `commit_msg_check.py` reading the diff
`git commit -v` writes below its scissors line as the message, an `ADR-NNNN` there passing it,
and taking any ADR named with any ADR staged. The owner decided, on 2026-11-02, that all of it
be fixed, and it was: a git command naming `core.hooksPath` passes only to install the hooks or read the setting;
a quoted word is read unquoted; `git` is found as a path, quoted, and after `env`, `sudo` or
`command` with their options; every option before `push` is read, its argument with it, and
only `-C` passes; the rest of a heredoc operator's line is read; and the message is read up to
the scissors line and must name an ADR that is staged. Shown to fire: of 47 commands, the
guard before this amendment passed the ten above, and the new one blocks the 30 it should and
passes the 17 it should; of four messages, the old check passed one naming ADR-0005 with
ADR-0003 staged and one naming an ADR only below the scissors, and the new one fails both.

## Amendment: SVGs scanned, and HTML links

- **Date**: 2026-11-02

With the logo's SVGs added, a further outside review found that betterleaks' own default
allow-list skips every path ending `.svg`, that `documents.py` and `paths.py` read no SVG,
and that `documents.py` checked Markdown links alone, not the `src` and `srcset` of the
README's HTML. The owner decided, on 2026-11-02, that it be fixed: the pre-commit hook and
the audit also pass each SVG, staged, in the tree or in the history, to `betterleaks
stdin`, where no path is matched; `documents.py` and `paths.py` read SVGs; and a
document's HTML `src`, `srcset` and `href` must resolve. Shown to fire: a staged SVG
holding a key passed the old hook and failed the new; an SVG holding Japanese and a path
leading outside the project, and a README naming a logo that does not exist, passed the
old checks and failed the new. It also reported that `env -u VAR git …`, `export …; git …`
and a `git` after a pipe passed the guard, which blocked each already.

## Amendment: the guard a convenience, the backstop whole

- **Date**: 2026-11-02

The survey of the whole repository found the guards run on a list of tool names, so that a
tool added later, such as one running a shell command, was unguarded; `claude_guard.py`
reading the shell with regular expressions, so that `--no-''verify`, `\git`, `nice git`,
`HOME=… git`, `git config include.path` and `git commit-tree` passed; the backstop it named,
the history scan, checking none of what `commit_msg_check.py` checks, and commit messages
scanned with the personal rules alone; and betterleaks' default allow-list skipping
`node_modules/`, `.gltf` and more by name, not `.svg` alone. The owner decided, on
2026-11-02, that it be fixed at its cause: both guards run on every tool; the guard splits
words as the shell does, finds `git` wherever it stands in a command, and stops git's
configuration changed by the environment, an include, a remote's URL and plumbing that
runs no hook, as a convenience; the backstop is the audit, which holds each commit not yet
pushed, and on GitHub each commit a push brought, to `commit_msg_check.py`, and every
message to the project's rules; and every text file git would commit, staged or in the
history, is passed to `betterleaks stdin` by `leak_input.py`, where no path is matched.
Shown to fire: the old guard passed each command above, and of 63 the new one blocks the
43 it should and passes the 20 it should; a commit of the history, made before the hook held an ADR to its
number, fails `--commits`; staged files under `node_modules/` and ending `.gltf` passed
betterleaks and fail through standard input.
