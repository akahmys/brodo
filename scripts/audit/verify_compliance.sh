#!/bin/bash
# The gate (AUDITING.md). Runs every mechanical check on CODING.md and TESTING.md, then the
# tests, and ends `=== AUDIT PASSED ===` or `=== AUDIT FAILED ===`. Each step is one
# `echo "[...]"` line, so the step list in AUDITING.md can be derived from this file.
set -u
cd "$(dirname "$0")/../.." || exit 1

# A neutral environment. An empty RUSTFLAGS overrides `rustflags` in any cargo
# configuration, the user's included, which could otherwise switch a lint off (Rule 19).
export RUSTFLAGS=""
unset CARGO_ENCODED_RUSTFLAGS CLIPPY_CONF_DIR RUSTDOCFLAGS

ERROR=0
fail() { echo "  FAIL: $*"; ERROR=1; }
# On GitHub Actions the two checks that read files kept on the owner's machine alone, the
# local list of forbidden sources and the personal rules, cannot run; each says so, and the
# run before a push, on that machine, still runs them (ADR-0034).
REMOTE=0
[ "${GITHUB_ACTIONS:-}" = "true" ] && REMOTE=1
# Each compiler error with its location, without the warnings around it.
errors_only() { awk '/^error/ { e = $0; next } e && /^ *--> / { print e; print; e = "" }' | head -40; }
for tool in cargo cargo-deny cargo-mutants cargo-semver-checks betterleaks python3 git; do
    command -v "$tool" >/dev/null 2>&1 || { echo "missing tool: $tool"; exit 1; }
done
# The project's own settings (scripts/audit/project.py).
setting() { python3 scripts/audit/project.py "$1"; }
UNTRUSTED_INPUT=$(setting UNTRUSTED_INPUT)
TARGETS=$(setting TARGETS)
SRC_DIRS=$( (find crates -maxdepth 2 -type d -name src; [ -d xtask/src ] && echo xtask/src) | sort | tr '\n' ' ')
MEMBERS=$(cargo metadata --no-deps --format-version 1 \
    | python3 -c "import sys,json;print(' '.join(p['name'] for p in json.load(sys.stdin)['packages']))")

echo "=== audit ==="

echo "[Toolchain] rust-version, the pin and the running compiler agree..."
pin=$(sed -nE 's/^channel = "([0-9.]+)"/\1/p' rust-toolchain.toml)
stated=$(sed -nE 's/^rust-version = "([0-9.]+)"/\1/p' Cargo.toml)
running=$(rustc --version | awk '{print $2}')
if [ "${pin%.*}" != "$stated" ]; then fail "rust-version $stated, pin $pin"
elif [ "$running" != "$pin" ]; then fail "rustc $running runs, $pin is pinned"
else echo "  PASS ($pin)"; fi

echo "[Rules 4, 11, 19, 20, L1, L3, L4, V1, V3, T6] Workspace members, lint inheritance, layer order, version, fuzzer, layout..."
python3 scripts/audit/workspace.py || ERROR=1

echo "[Rules 1, 4, 6, 7, 9, 10, 16, 17, 18, 19] cargo clippy --workspace --all-targets -D warnings..."
clippy_out=$(cargo clippy --workspace --all-targets --quiet -- -D warnings 2>&1)
clippy_status=$?
# clippy.toml paths that resolve to nothing are warnings `-D warnings` does not promote.
if grep -q "does not refer to a" <<<"$clippy_out"; then
    grep -A2 "does not refer to a" <<<"$clippy_out"; fail "clippy.toml names a path that does not exist"
elif [ $clippy_status -ne 0 ]; then
    errors_only <<<"$clippy_out"; fail "clippy"
else echo "  PASS"; fi

# Code behind a cargo feature is linted, and so built, with no default features and with each
# feature alone, not only with the defaults the step above takes.
echo "[Rule 10] cargo clippy with no default features and with each feature alone..."
feature_sets=""; ERROR_BEFORE=$ERROR; ERROR=0
for member in $MEMBERS; do
    features=$(cargo metadata --no-deps --format-version 1 | python3 -c "
import sys, json
for p in json.load(sys.stdin)['packages']:
    if p['name'] == '$member':
        print(' '.join(f for f in p['features'] if f != 'default'))")
    for set in "" $features; do
        if ! out=$(cargo clippy -p "$member" --all-targets --no-default-features \
                ${set:+--features "$set"} --quiet -- -D warnings 2>&1); then
            errors_only <<<"$out"; fail "clippy -p $member --no-default-features ${set:+--features $set}"
        fi
        feature_sets+=" $member[${set:-none}]"
    done
done
[ $ERROR -eq 0 ] && echo "  PASS ($feature_sets )"
[ $ERROR_BEFORE -eq 0 ] || ERROR=1

# Rules 2 and 3 apply to production code only, so they run apart from the clippy step above,
# with every feature, so that a module behind one is read (ADR-0021).
# The workspace lints fire here too; lint_filter.py keeps each step to its own lints.
# Both warn rather than deny their lints and fail if clippy does not finish: a finding
# that stops the build of one crate would otherwise leave the crates above it unlinted,
# and the step would pass having checked them not at all (ADR-0002).
echo "[Rule 2] No panic in production code..."
if json=$(cargo clippy --workspace --lib --bins --all-features --quiet --message-format=json -- \
        -W clippy::unwrap_used -W clippy::expect_used -W clippy::panic \
        -W clippy::unreachable -W clippy::todo -W clippy::unimplemented 2>/dev/null); then
    python3 scripts/audit/lint_filter.py clippy::unwrap_used clippy::expect_used \
        clippy::panic clippy::unreachable clippy::todo clippy::unimplemented <<<"$json" || ERROR=1
else fail "clippy did not finish on the workspace"; fi

echo "[Rules 2, 4, 15, 18, 19] No assert!, unsafe only in the listed module, no recursion without a stated bound, exceptions only for listed lints..."
python3 scripts/audit/source.py || ERROR=1

echo "[Rule 3] No indexing or slicing in the modules that parse untrusted input..."
checked=""
for module in $UNTRUSTED_INPUT; do
    crate=$(cut -d/ -f2 <<<"$module")
    if [ ! -e "$module" ]; then fail "$module (project.py) does not exist"; continue; fi
    if ! grep -qw "$crate" <<<"$MEMBERS"; then fail "$crate (project.py) is not a member"; continue; fi
    if json=$(cargo clippy -p "$crate" --lib --bins --all-features --quiet --message-format=json -- \
            -W clippy::indexing_slicing 2>/dev/null); then
        python3 scripts/audit/lint_filter.py --under "$module" \
            clippy::indexing_slicing <<<"$json" || ERROR=1
    else fail "clippy did not finish on $crate"; fi
    checked+=" $module"
done
[ -n "$checked" ] || echo "  PASS (no module is listed in project.py)"

# `String` as `Result`'s own error however its types nest, so that
# `Result<BTreeMap<String, String>, E>` passes, and errors filtered away, each however its
# lines break (ADR-0002).
echo "[Rules 8, 9] No String or anyhow errors in library crates, no errors filtered away..."
python3 scripts/audit/source.py --errors || ERROR=1

echo "[Rule 10] cargo fmt --all --check..."
if cargo fmt --all --check >/dev/null 2>&1; then echo "  PASS"
else cargo fmt --all --check 2>&1 | grep '^Diff in' | sort -u; fail "run 'cargo fmt --all'"; fi

# Rule 12: a crate that compiles C pulls `cc`. A tree cargo cannot give fails, rather than
# passing as one without `cc` (ADR-0005).
echo "[Rule 12] No dependency compiles C ($TARGETS)..."
c_builders=""; ERROR_BEFORE=$ERROR; ERROR=0
for member in $MEMBERS; do
    for triple in $TARGETS; do
        if ! tree=$(cargo tree -p "$member" --target "$triple" --all-features --edges normal,build \
                --prefix none 2>&1); then
            fail "cargo tree -p $member --target $triple"
        elif grep -q '^cc v' <<<"$tree"; then
            c_builders+=" $member($triple)"
        fi
    done
done
[ -z "$c_builders" ] || fail "these pull cc:$c_builders (cargo tree -p <crate> --target <triple> -i cc)"
[ $ERROR -eq 0 ] && echo "  PASS"
[ $ERROR_BEFORE -eq 0 ] || ERROR=1

echo "[T1] Tests: no test file in src/, and #[ignore] only as needing what the repository lacks..."
stray=$(find $SRC_DIRS \( -name '*_test.rs' -o -name '*_tests.rs' -o -name 'test_*.rs' \
    -o -name 'tests.rs' -o -name 'test.rs' \) 2>/dev/null)
[ -z "$stray" ] || { echo "$stray"; fail "test files in src/"; }
# An ignore written directly or under `cfg_attr`, however its lines break, in any target.
python3 scripts/audit/source.py --ignores || ERROR=1

echo "[Docs, S3] Language, tense, links, ADR index, provenance record, rustdoc..."
python3 scripts/audit/documents.py || ERROR=1
if ! out=$(RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --quiet 2>&1); then
    echo "$out" | head -20; fail "cargo doc"
fi

echo "[T11] Every mutant excluded as equivalent says why, and still exists..."
python3 scripts/audit/mutants.py || ERROR=1

echo "[V9] From 1.0, no break of the last release's API or of mestolo's command line beyond what the version allows..."
python3 scripts/audit/semver.py || ERROR=1

echo "[Rule 13] cargo deny check (licences, advisories, bans, sources)..."
if out=$(cargo deny check 2>&1); then echo "  PASS"
else echo "$out" | grep -E "^error" | head -20; fail "cargo deny"; fi

echo "[S1] The local list of forbidden sources the guard reads..."
if [ $REMOTE -eq 1 ]; then echo "  SKIPPED on GitHub Actions: the list stays on the owner's machine"
else python3 scripts/hooks/provenance_guard.py --check || ERROR=1; fi

echo "[Rules 14, L2] Secrets, personal data and paths outside the project..."
ERROR_BEFORE=$ERROR; ERROR=0
[ "$(git config core.hooksPath)" = "scripts/hooks" ] \
    || fail "the commit hooks are not installed: git config core.hooksPath scripts/hooks"
# betterleaks reads `.betterleaks.toml` only when given it (`-c`), from 1.10.0, so every
# scan by the project's rules names it, and the project's rules are counted beyond the
# default ones its `[extend]` loads (ADR-0003).
count() { betterleaks config check "$@" 2>&1 | sed -nE 's/^OK: ([0-9]+) rules.*/\1/p'; }
default=$(count); rules=$(count -c .betterleaks.toml)
[ "${rules:-0}" -gt "${default:-0}" ] \
    || fail "betterleaks loaded ${rules:-no} rules from .betterleaks.toml, its default ${default:-none}; is [extend] or a rule of the project's missing?"
betterleaks dir . --no-banner --redact -c .betterleaks.toml >/dev/null 2>&1 \
    || fail "betterleaks: run 'betterleaks dir . --no-banner --redact -c .betterleaks.toml'"
# betterleaks' default allow-list skips paths by name (`node_modules/`, `.svg`, …); read
# through standard input, every text file git would commit, and every text blob of the
# history, is scanned whatever its name (ADR-0003).
everything() { python3 scripts/audit/leak_input.py --tree; python3 scripts/audit/leak_input.py --history; }
everything | betterleaks stdin --no-banner --redact -c .betterleaks.toml >/dev/null 2>&1 \
    || fail "a secret or a path outside the project, read through standard input: run 'python3 scripts/audit/leak_input.py --tree | betterleaks stdin'"
out=$(python3 scripts/audit/paths.py) || { echo "$out"; ERROR=1; }
# The commits the hooks may have been skipped for: on GitHub those a push brought
# (`$COMMIT_RANGE`, set by the workflow), here those not yet pushed. Each is held to the
# commit-msg check, and every message to the project's rules (ADR-0003).
range="${COMMIT_RANGE:-}"
if [ -z "$range" ] && git rev-parse -q --verify '@{upstream}' >/dev/null; then range='@{upstream}..HEAD'; fi
case "$range" in 0000000000000000000000000000000000000000..*) range="${range#*..}^!";; esac
if [ -n "$range" ] && git rev-parse --verify -q HEAD >/dev/null; then
    commits=$(git rev-list "$range")
    if [ -n "$commits" ]; then
        # shellcheck disable=SC2086
        out=$(python3 scripts/hooks/commit_msg_check.py --commits $commits) \
            || { echo "$out"; fail "a commit made breaks the commit-msg check"; }
    fi
fi
if git rev-parse --verify -q HEAD >/dev/null; then
    git log --format='%B' | betterleaks stdin --no-banner --redact -c .betterleaks.toml >/dev/null 2>&1 \
        || fail "a secret or a path outside the project in a commit message"
fi
PERSONAL="${PERSONAL_LEAKS:-$HOME/.config/betterleaks/personal.toml}"
if [ $REMOTE -eq 1 ]; then
    echo "  SKIPPED on GitHub Actions: the personal rules stay on the owner's machine"
    if git rev-parse --verify -q HEAD >/dev/null; then
        betterleaks git . --no-banner --redact -c .betterleaks.toml >/dev/null 2>&1 \
            || fail "a secret or a path outside the project in the history"
    fi
elif [ ! -f "$PERSONAL" ]; then
    fail "no personal rules at \$PERSONAL_LEAKS or the default (AUDITING.md §2)"
else
    betterleaks dir . --no-banner --redact -c "$PERSONAL" >/dev/null 2>&1 \
        || fail "personal data in the working tree"
    everything | betterleaks stdin --no-banner --redact -c "$PERSONAL" >/dev/null 2>&1 \
        || fail "personal data, read through standard input"
    if git rev-parse --verify -q HEAD >/dev/null; then
        # The history too: a commit made with --no-verify skipped the hooks.
        betterleaks git . --no-banner --redact -c .betterleaks.toml >/dev/null 2>&1 \
            || fail "a secret or a path outside the project in the history"
        betterleaks git . --no-banner --redact -c "$PERSONAL" >/dev/null 2>&1 \
            || fail "personal data in the history"
        git log --format='%an%n%ae%n%cn%n%ce%n%B' \
            | betterleaks stdin --no-banner --redact -c "$PERSONAL" >/dev/null 2>&1 \
            || fail "personal data in a commit message or author"
    fi
fi
[ $ERROR -eq 0 ] && echo "  PASS"
[ $ERROR_BEFORE -eq 0 ] || ERROR=1

echo "[Gate] cargo test --workspace --all-features..."
if out=$(cargo test --workspace --all-features --quiet 2>&1); then
    echo "  PASS ($(grep -oE 'test result: ok\. [0-9]+ passed' <<<"$out" \
        | awk '{s += $4} END {print s + 0}') tests)"
else echo "$out" | grep -E "^test .* FAILED|^error|panicked" | head -20; fail "cargo test"; fi

echo "[T2] Every test is in its crate's test-list.txt, and every listed test exists..."
python3 scripts/audit/tests.py || ERROR=1

if [ $ERROR -ne 0 ]; then echo "=== AUDIT FAILED ==="; exit 1; fi
echo "=== AUDIT PASSED ==="
