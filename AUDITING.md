# Auditing

> **Phase: audit.** What is checked mechanically, and by what. The rules themselves are in
> [CODING.md](CODING.md) and [TESTING.md](TESTING.md).

One script runs every check, then the tests, then the check that every test is listed: [`scripts/audit/verify_compliance.sh`](scripts/audit/verify_compliance.sh).
It must end `=== AUDIT PASSED ===`. It needs `cargo`, `cargo-deny`, `cargo-mutants`,
`betterleaks`, `python3` and `git`, and stops if one is missing. It runs with `RUSTFLAGS` empty, which
overrides `rustflags` in any cargo configuration, so no configuration outside the
repository can switch a lint off.

Not in it: the fuzzer and Miri, which build on nightly, and the mutation run, which takes
hours, its verdict by `scripts/audit/mutants.py` holding a run's or a sharded run's reports
to every mutant `cargo mutants --list` gives, to a baseline that passed and to the memory
guard's log; TESTING.md T6, T7 and T11 say when they run.
Nor the benchmark harness, which runs its children on as many threads as the machine has
performance cores and refuses a pair of runs that disagree on an item by more than 10 %
(TESTING.md T5); it runs with the benchmarks, under TESTING.md T10.

The project's own settings are in [`scripts/audit/project.py`](scripts/audit/project.py),
read by the script and by the checks it runs.

## 1. The steps

Derive the list rather than trusting this table:

```bash
grep -E '^echo "\[' scripts/audit/verify_compliance.sh
```

| Step | Rules |
| :--- | :--- |
| Toolchain: `rust-version`, `rust-toolchain.toml` and the running `rustc` agree | — |
| `scripts/audit/workspace.py`: members, lint inheritance, layer order, version and licence inheritance, the version's form, CHANGELOG.md's version, `rust-version` unchanged in a PATCH, dependency inheritance, `rustflags`, the fuzzer's reach, the root's entries against ARCHITECTURE.md §3, where scripts lie, where each crate is published, its dependency on brodo by version and what its package holds | 4, 11, 19, 20, L1, L3, L4, V1, V3, V7, V8, T6 |
| `cargo clippy --workspace --all-targets -- -D warnings`, with the lints in `Cargo.toml` and the bans in `clippy.toml` | 1, 4, 6, 7, 9, 10, 16, 17, 18, 19 |
| `cargo clippy --all-targets -D warnings` per member with no default features and with each feature alone | 10 |
| clippy's panic lints over `--lib --bins` with every feature, filtered by `scripts/audit/lint_filter.py` | 2 |
| `scripts/audit/source.py`, each file git would commit read by a lexer, its strings, characters and comments, nested, blanked first: asserts and recursion in production code, each file read up to its test module, `mod tests`, a body read from the line after its signature as `cargo fmt` writes it; every part of every `allow` and `expect`, directly or under `cfg_attr`, in every target and test module, a lint or its reason; `unsafe` outside `UNSAFE_MODULES`, in every file, but a call of `CacheFile::map` in `UNSAFE_CALLERS` | 2, 4, 15, 18, 19 |
| clippy's `indexing_slicing` over the crate of each module in `UNTRUSTED_INPUT`, with every feature, its findings kept to that module by `scripts/audit/lint_filter.py --under` | 3 |
| `scripts/audit/source.py --errors`: an error with no type of its own, a string or a boxed `dyn Error`, as `Result`'s, and `anyhow`, in a library crate, and `filter_map` or `flat_map` of `Result::ok` and its forms, however the types nest, an arrow among them, or the lines break | 8, 9 |
| `cargo fmt --all --check` | 10 |
| `cargo tree --all-features --edges normal,build` per member and target, searched for `cc`, failing where cargo gives no tree | 12 |
| test files in `src/`, and, by `scripts/audit/source.py --ignores`, `#[ignore]` reasons, direct or under `cfg_attr`, however their lines break, found in the code and not its strings, in every target | T1 |
| `scripts/audit/documents.py`, and `cargo doc` with every feature and warnings denied | AGENTS.md writing rules 1, 2, 6, S3, the ADR index, no `CLAUDE.md`, each crate's README read in, and no `#[doc(hidden)]` item named by another file's doc comment, a crate README, ARCHITECTURE.md or docs/formats.md; Rule 10 |
| `scripts/audit/mutants.py` with no argument: `.cargo/mutants.toml` holds no setting, read by its shape, one `exclude_re = [` list and comments, and each pattern in its `exclude_re` has a comment above it ending in its kind and, for `open` and `unreached`, where, and excludes a mutant | T11 |
| `cargo deny check` | 13 |
| `scripts/hooks/provenance_guard.py --check`: the local list of forbidden sources exists and names a project | S1 |
| hooks installed; betterleaks over the tree and the history, personal rules, `scripts/audit/paths.py` (also: each of `OUTPUT_DIRS` git-ignored and in betterleaks' allow-list) | 14, L2 |
| `cargo test --workspace --all-features`, a crate README's code among the doctests, and `format_document.rs` and `vdb_format.rs` running `scripts/audit/formats.py` | TESTING.md §4; AGENTS.md, crate READMEs; `docs/formats.md` and TESTING.md T13 (ADR-0037) |
| `scripts/audit/tests.py`: every test, with every feature, of each member's library, binaries, tests and examples, and every doctest by its item, is in its `test-list.txt`, and every listed test exists | T2 |

**Two traps the script guards against.** A path in `clippy.toml` that resolves to nothing
is a warning `-D warnings` does not promote, so the ban silently does nothing; the script
fails on it. A crate without `[lints] workspace = true` escapes every lint, `unsafe_code`
included; `workspace.py` fails on it.

**A step that finds nothing must be shown to find something.** After changing a check,
break the rule once and watch the step fail (TESTING.md T3).

## 2. Secrets, personal data, and paths outside the project

[`betterleaks`](.betterleaks.toml) runs in three places:

| Where | Reads |
| :--- | :--- |
| `scripts/hooks/pre-commit` | the staged change, and every staged text file through standard input (`leak_input.py`); also runs `paths.py` |
| `scripts/hooks/commit-msg` | the commit message; `commit_msg_check.py`, reading it up to the scissors line of `git commit -v`, also requires English, CJK punctuation refused, and a staged ADR named for a rule change (AGENTS.md), a lint level in `Cargo.toml` alone or in a table and a rule file renamed away included, or a test removed from a `test-list.txt` (TESTING.md T2), and a `Benchmarks` line for a change to the `src/` of a crate with a benchmark, its last report newer than the change and every item `ok` unless the line says the path is not timed (TESTING.md T10), and ROADMAP.md §1 changed by a commit that adds an ADR or ticks a box (PLANNING.md §1) |
| the audit | the working tree and the whole history, and every text file and blob of either through standard input (`leak_input.py`), and every commit message; with the personal rules also every commit message and author |

**On GitHub Actions** (`.github/workflows/audit.yml`, started by each push to `main` and by hand) the gate skips the
local list of forbidden sources and the personal rules, which stay on the owner's machine,
and says so; the gate on that machine before every push runs them (ADR-0034). The same
workflow runs the gate's tests on macOS and Windows, where the rest of the gate, which does
not depend on the platform, is not run.

**Getting round the hooks.** The audit fails while `core.hooksPath` is not `scripts/hooks`.
In Claude Code, [`.claude/settings.json`](.claude/settings.json) runs
`scripts/hooks/claude_guard.py` before every tool's call, reading the shell command any
names, which blocks, as a convenience, skipping the hooks, `--no-verify` abbreviated
included, a change to `core.hooksPath`, git's configuration changed by `-c`,
`GIT_CONFIG_*`, `HOME` or `XDG_CONFIG_HOME`, an alias, an include or a remote's URL set,
plumbing that makes a commit or a ref, and every push but `git push origin` of `main` and
version tags `vX.Y.Z`, with no option but `-C` (ADR-0003). It splits the command into words
as the shell does and finds `git` wherever it stands in a command. A command written another
way can pass the guard. The backstop is the audit: each commit not yet pushed, and on
GitHub each commit a push brought (`COMMIT_RANGE`), is held to `commit_msg_check.py
--commits`, and every commit message to the project's rules.

**Forbidden sources.** The same file runs `scripts/hooks/provenance_guard.py` before every
Read, Grep, Glob, WebFetch, WebSearch, NotebookEdit, Bash and MCP tool call, a terminal's
or a browser's among them, reading every string of the call's input, which blocks a path,
URL or command naming a source AGENTS.md S1 forbids: OpenVDB but at a tag from `v12.0.0` on, OpenFOAM, and the names in the owner's local
list, kept outside the repository so that it names no other project of the owner's. It
matches names only, so no directory on any machine is written into the repository: a name
after a `/`, or after a `:` or `=` joining a path (`<tag>:<name>/…`, `--dir=<name>`).
OpenVDB passes where a path or URL names a tag from `v12.0.0` on, a segment `vX.Y.Z` among
the two after its name or a directory `openvdb-vX.Y.Z`, the repository's own directories
under it included; a path that climbs with `..` is judged segment by segment, and a path
ends at white space, a quote or a shell's punctuation (ADR-0044).
Nothing backs it up: what was read leaves no trace in the tree.

Install the hooks once per clone:

```bash
git config core.hooksPath scripts/hooks
```

**Everything fails closed.** A missing `betterleaks` or a missing personal rule file stops
the commit and fails the audit; nothing is skipped silently.

**`.betterleaks.toml` extends the default rules** (`[extend] useDefault = true`); without
that line the file's own rules are the only ones. betterleaks reads the file only when
given it, `-c .betterleaks.toml`, from 1.10.0, so every scan by the project's rules names
it, and the audit fails unless more rules load from it than betterleaks' default holds. It adds one rule, `path-outside-project`, for absolute paths into a home or
temporary directory. A relative path that leads outside the root is found by
`scripts/audit/paths.py`, which resolves each relative path against the file holding it.
The file's path allow-list skips what `.gitignore` ignores at the root: `target/` and
`out/`, the directories of CODING.md L2, the spikes' and the fuzzer's `target/`, `.git/`
and `.claude/settings.local.json`; the same name deeper in the tree is scanned, and
`paths.py` and `documents.py` read every text file git would commit, whatever its name.
betterleaks' own default allow-list skips paths by name (`node_modules/`, `.svg`, `.gltf`
and more), so the hook and the audit also pass every text file to `betterleaks stdin`,
where no path is matched.

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
