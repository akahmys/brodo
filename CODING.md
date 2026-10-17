# Coding

> **Phase: implementation.** What code must satisfy. What to decide first is in
> [PLANNING.md](PLANNING.md); what to run afterwards is in [TESTING.md](TESTING.md).

"Production code" is everything under `crates/*/src/` and `xtask/src/` outside `#[cfg(test)]`.
An exception is written as Rule 19 says, and only where a rule allows one.
The crates and modules each rule names (layers, application crates, modules that parse
untrusted input, the unsafe module, targets) are listed once, in
[`scripts/audit/project.py`](scripts/audit/project.py).

## 1. Rules

| Rule | Requirement | Enforced by |
| :--- | :--- | :--- |
| **1** | A function is at most 50 lines. Exception: by `#[expect]` | clippy `too_many_lines` |
| **2** | No `unwrap`, `expect`, `panic!`, `unreachable!`, `todo!`, `unimplemented!`, `assert!`, `assert_eq!` or `assert_ne!` in production code. Use `?` and typed errors; state an invariant with `debug_assert!`. Exception: by `#[expect]`, with a reason that states the invariant | clippy, and `scripts/audit/source.py` for the asserts |
| **3** | No indexing or slicing (`a[i]`, `&a[i..j]`) in a module that parses untrusted bytes (a file, a mesh), listed in `UNTRUSTED_INPUT` by the commit that creates it (ADR-0009). Elsewhere code may index through ids validated when they were made. Such a module is also fuzzed (TESTING.md T6) | clippy `indexing_slicing`, in the audit; that a parsing module is listed, **nothing** |
| **4** | No `unsafe`, and so no `static mut`, except in the one file `UNSAFE_MODULES` lists: brodo's memory mapping (ADR-0008). There, the file carries `#![expect(unsafe_code, reason = "…")]`, each `unsafe` block holds one operation under a `// SAFETY:` comment, and its tests run under Miri (TESTING.md T7) | rustc `unsafe_code = "deny"`; clippy `undocumented_unsafe_blocks`, `multiple_unsafe_ops_per_block`; `scripts/audit/source.py` for where; the audit fails a crate that does not inherit the workspace lints |
| **5** | A structural absence is a type, not a sentinel or an `Option` | nothing |
| **6** | No `HashMap` or `HashSet`: their iteration order varies between runs. Use `BTreeMap`, `BTreeSet` or a `Vec` | clippy `disallowed_types` |
| **7** | No wildcard arm (`_ =>`) in a `match` over an enum, so a new variant breaks every decision on it. Exception: an external `#[non_exhaustive]` enum, by `#[expect]` | clippy `wildcard_enum_match_arm` and `match_wildcard_for_single_variants` |
| **8** | Library crates return typed errors (`thiserror`). No `Result<_, String>` and no `anyhow`, except in an application crate (`APPLICATIONS`) and `xtask` | grep, in the audit |
| **9** | No error discarded: no `let _ =` on a `#[must_use]` value, no `filter_map(Result::ok)` | clippy `let_underscore_must_use`; grep |
| **10** | `cargo fmt --all --check` passes; the build and clippy's default lints give no warning (`too_many_arguments` may be excepted); every public item has a doc comment, and every doc link resolves | the audit: `cargo fmt`, `cargo clippy -D warnings` with rustc `missing_docs`, `cargo doc -D warnings` |
| **11** | A crate depends only on crates below it in `LAYERS`, in every dependency kind. `xtask` may depend on anything; nothing depends on it | `scripts/audit/workspace.py` |
| **12** | Pure Rust: no dependency compiles C or C++ (none pulls `cc`) for any of `TARGETS`. Exception: by ADR, naming the crate that compiles C | `cargo tree`, in the audit |
| **13** | A dependency's licence is on `deny.toml`'s allow-list, which holds permissive licences only (ADR-0005); it has no known advisory and comes from crates.io | `cargo deny check` |
| **14** | No secret, no personal data, and no path outside the project (absolute or relative) is committed | betterleaks and `scripts/audit/paths.py` ([AUDITING.md](AUDITING.md) §2) |
| **15** | No recursion in production code; walk with an explicit stack (`Vec`). Exception: `// Bounded recursion: <the bound>` among the comments above the `fn`, naming what limits the depth | `scripts/audit/source.py`, from a call graph built on names: it misses recursion through closures and trait objects |
| **16** | Every loop that iterates toward a condition (a retry, a convergence, a poll) stops at a maximum count from its configuration, and ending without reaching the condition is a typed result | clippy `infinite_loop` catches a loop with no exit; the bound itself, **nothing** |
| **17** | No `as` cast that can truncate, wrap or lose the sign (use `try_from`), and no `==` or `!=` between two floats (comparing with a literal zero is allowed). Exception: by `#[expect]` | clippy `cast_possible_truncation`, `cast_possible_wrap`, `cast_sign_loss`, `float_cmp` |
| **18** | No `dbg!` and no `print!`-family macros. An application crate and `xtask` may print, by a crate-level `#![expect]` | clippy `dbg_macro`, `print_stdout`, `print_stderr`; `scripts/audit/source.py` for where |
| **19** | An exception is `#[expect(lint, reason = "…")]`, never `#[allow]`, so it fails once nothing needs it. It covers only the lints of Rules 1, 2, 4 (`unsafe_code`, in its one file), 7, 10 (`too_many_arguments`), 17 and 18. No `rustflags` in a cargo configuration | clippy `allow_attributes`, `allow_attributes_without_reason`; `scripts/audit/source.py` for the lint list; `scripts/audit/workspace.py` for `rustflags`, and the audit runs with `RUSTFLAGS` empty |
| **20** | Every dependency is declared once in `[workspace.dependencies]` and inherited with `workspace = true`. A new external dependency is approved by the human before it is added | `scripts/audit/workspace.py`; the approval, **nothing** |
| **21** | Input or an option the project does not support is a typed error that names it, never a silent default | nothing |

## 2. Where files go

The directories are listed in ARCHITECTURE.md §3.

| Rule | Requirement | Enforced by |
| :--- | :--- | :--- |
| **L1** | Every workspace member is in `crates/`, or is `xtask/`. `spikes/` and `fuzz/` are never members (ADR-0006) | `scripts/audit/workspace.py` |
| **L2** | Output, and the environments tools build (a Python virtual environment and the like), go under `target/` or `out/`, both git-ignored | `scripts/audit/paths.py`: the directories are listed once, as `OUTPUT_DIRS` in `scripts/audit/project.py`, and each is git-ignored and in `.betterleaks.toml`'s allow-list; the document, path and secret scans skip only those, and fail on files elsewhere. That output goes there, **nothing** |
| **L3** | The root holds the documents AGENTS.md lists, the licence files, workspace and tool configuration, and `.claude/settings.json`. A new root directory is added to ARCHITECTURE.md §3 | nothing |
| **L4** | A script lives in `scripts/audit/` (checks), `scripts/hooks/` (git and Claude Code hooks), or is an `xtask` subcommand | nothing |
