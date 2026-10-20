# Auditing

> **Phase: audit.** What is checked mechanically, and by what. The rules themselves are in
> [CODING.md](CODING.md) and [TESTING.md](TESTING.md).

One script runs everything, the tests last: [`scripts/audit/verify_compliance.sh`](scripts/audit/verify_compliance.sh).
It must end `=== AUDIT PASSED ===`. It needs `cargo`, `cargo-deny`, `cargo-mutants`,
`betterleaks`, `python3` and `git`, and stops if one is missing. It runs with `RUSTFLAGS` empty, which
overrides `rustflags` in any cargo configuration, so no configuration outside the
repository can switch a lint off.

Not in it: the fuzzer and Miri, which build on nightly, and the mutation run, which takes
hours; TESTING.md T6, T7 and T11 say when they run.

The project's own settings (layers, application crates, modules that parse untrusted
input, the unsafe module, targets) are in [`scripts/audit/project.py`](scripts/audit/project.py),
read by the script and by `workspace.py` and `source.py`.

## 1. The steps

Derive the list rather than trusting this table:

```bash
grep -E '^echo "\[' scripts/audit/verify_compliance.sh
```

| Step | Rules |
| :--- | :--- |
| Toolchain: `rust-version`, `rust-toolchain.toml` and the running `rustc` agree | — |
| `scripts/audit/workspace.py`: members, lint inheritance, layer order, version and licence inheritance, CHANGELOG.md's version, dependency inheritance, `rustflags`, the fuzzer's reach | 4, 11, 19, 20, L1, V1, V3, T6 |
| `cargo clippy --workspace --all-targets -- -D warnings`, with the lints in `Cargo.toml` and the bans in `clippy.toml` | 1, 4, 6, 7, 9, 10, 16, 17, 18, 19 |
| `cargo clippy --all-targets -D warnings` per member with no default features and with each feature alone | 10 |
| clippy's panic lints over `--lib --bins`, filtered by `scripts/audit/lint_filter.py` | 2 |
| `scripts/audit/source.py`: asserts and recursion in production code; the lints an exception names, in every target; `unsafe` outside `UNSAFE_MODULES`, in every file | 2, 4, 15, 18, 19 |
| clippy's `indexing_slicing` over the crate of each module in `UNTRUSTED_INPUT`, its findings kept to that module by `scripts/audit/lint_filter.py --under` | 3 |
| grep for `String` and `anyhow` errors | 8 |
| grep for `filter_map(Result::ok)` and its forms | 9 |
| `cargo fmt --all --check` | 10 |
| `cargo tree --all-features … -i cc` per member and target | 12 |
| test files in `src/`, and `#[ignore]` reasons, direct or under `cfg_attr`, in every target | T1 |
| `scripts/audit/documents.py`, and `cargo doc` with warnings denied | AGENTS.md writing rules 1, 2, 6, S3, and no `CLAUDE.md`; Rule 10 |
| `scripts/audit/mutants.py`: `.cargo/mutants.toml` holds no setting, and each pattern in its `exclude_re` has a comment above it and excludes a mutant | T11 |
| `cargo deny check` | 13 |
| hooks installed; betterleaks over the tree and the history, personal rules, `scripts/audit/paths.py` (also: each of `OUTPUT_DIRS` git-ignored and in betterleaks' allow-list) | 14, L2 |
| `cargo test --workspace --all-features`, a crate README's code among the doctests | TESTING.md §4; AGENTS.md, crate READMEs |
| `scripts/audit/tests.py`: every test, with every feature, of each member's library, binaries, tests and examples is in its `test-list.txt`, and every listed test exists | T2 |

**Two traps the script guards against.** A path in `clippy.toml` that resolves to nothing
is a warning `-D warnings` does not promote, so the ban silently does nothing; the script
fails on it. A crate without `[lints] workspace = true` escapes every lint, `unsafe_code`
included; `workspace.py` fails on it.

**Two lists start empty.** `UNTRUSTED_INPUT` and `UNSAFE_MODULES` are empty until the
modules exist, so Rules 3, 4 and T6 check nothing beyond the ban until then; listing a
module is part of the commit that creates it.

**A step that finds nothing must be shown to find something.** After changing a check,
break the rule once and watch the step fail (TESTING.md T3).

## 2. Secrets, personal data, and paths outside the project

[`betterleaks`](.betterleaks.toml) runs in three places:

| Where | Reads |
| :--- | :--- |
| `scripts/hooks/pre-commit` | the staged change; also runs `paths.py` |
| `scripts/hooks/commit-msg` | the commit message; `commit_msg_check.py` also requires English, and an ADR for a rule change (AGENTS.md) or a test removed from a `test-list.txt` (TESTING.md T2) |
| the audit | the working tree and the whole history; with the personal rules also every commit message and author |

**Getting round the hooks.** The audit fails while `core.hooksPath` is not `scripts/hooks`.
In Claude Code, [`.claude/settings.json`](.claude/settings.json) runs
`scripts/hooks/claude_guard.py` before every shell command, which blocks skipping the
hooks, a change to `core.hooksPath`, and pushing. A command written another way can pass
the guard; the history scan is the backstop.

**Forbidden sources.** The same file runs `scripts/hooks/provenance_guard.py` before every
read, search, fetch and shell command, which blocks a path or URL naming a source AGENTS.md
S1 forbids. It matches names only, so no directory on any machine is written into the
repository. Nothing backs it up: what was read leaves no trace in the tree.

Install the hooks once per clone:

```bash
git config core.hooksPath scripts/hooks
```

**Everything fails closed.** A missing `betterleaks` or a missing personal rule file stops
the commit and fails the audit; nothing is skipped silently.

**`.betterleaks.toml` extends the default rules** (`[extend] useDefault = true`); without
that line the file's own rules are the only ones. The audit fails when fewer than 100
rules load. It adds one rule, `path-outside-project`, for absolute paths into a home or
temporary directory. A relative path that leads outside the root is found by
`scripts/audit/paths.py`, which resolves each relative path against the file holding it.
The file's path allow-list skips `target/` and `out/`, the git-ignored directories of
CODING.md L2, as `paths.py` and `documents.py` do.

**Personal data** (the owner's name) is matched by rules kept outside the repository,
because a committed rule naming a person leaks the name. The file is `$PERSONAL_LEAKS`,
or by default `~/.config/betterleaks/personal.toml`.

## 3. Licences and advisories (`cargo-deny`)

[`deny.toml`](deny.toml) is the one place the licence allow-list is written: permissive
licences only (ADR-0005). `cargo deny check` also fails on a known advisory, an unknown
registry or git source, and a wildcard version. Advisories need the network to refresh
their database.

An *unmatched license allowance* warning means no dependency uses that licence. It is
expected while the dependency tree is small.
