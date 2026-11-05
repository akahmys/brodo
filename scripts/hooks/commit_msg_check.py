#!/usr/bin/env python3
"""Checks a commit message against the staged change (run by `commit-msg`).

1. The message is in English (AGENTS.md writing rule 6).
2. A change to a rule carries its record (AGENTS.md, "Changing the rules"): when a staged
   file states or enforces a rule, the message names an ADR that is staged too.
3. A test deleted or renamed carries its record (TESTING.md T2): when a line leaves a
   crate's `test-list.txt`, the message names an ADR that is staged too.
4. A change to the `src/` of a crate with a benchmark states its benchmarks (TESTING.md
   T10): a line of the message begins `Benchmarks`. In the hook alone, unless the line is
   `Benchmarks: not on a timed path`, the crate's `out/bench/<crate>/report.tsv` is newer
   than each of its staged `src/` files and gives every item `ok`: `out/` is not
   committed, so the audit cannot read it.
5. A unit of work that ends rewrites the status (PLANNING.md §1): a commit that adds an
   ADR or ticks a box in ROADMAP.md changes ROADMAP.md §1.

The message is read as git keeps it: comment lines aside, and nothing from the scissors
line on, below which `git commit -v` shows the staged diff.

With `--commits SHA…`, the same of each commit made, against its parent: the audit runs it
on the commits not yet pushed, and on GitHub on the commits a push brought, so that one
made without the hooks is caught (ADR-0003).
"""

import re
import subprocess
import sys
from pathlib import Path
from typing import Optional

# The audit's own lists, so that a commit is held to the same as the repository.
sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "audit"))
from paths import JAPANESE  # noqa: E402
from project import RULE_FILES as RULE_PATTERN  # noqa: E402

ADR_NAME = re.compile(r"\bADR-(\d{4})\b")
ADR_FILE = re.compile(r"^docs/adr/(\d{4})-")
SCISSORS = re.compile(r"^# -+ >8 -+$", re.MULTILINE)
RULE_FILES = re.compile(RULE_PATTERN)
TEST_LIST = re.compile(r"^crates/[^/]+/test-list\.txt$")
SOURCE = re.compile(r"^crates/([^/]+)/src/")
BENCHMARKS = re.compile(r"^Benchmarks\b", re.MULTILINE)
UNTIMED = re.compile(r"^Benchmarks: not on a timed path$", re.MULTILINE)
ROOT = Path(__file__).resolve().parent.parent.parent
TICKED = re.compile(r"^\s*- \[x\]", re.MULTILINE)
STATUS = re.compile(r"^## 1\. .*?(?=^## 2\. )", re.MULTILINE | re.DOTALL)
REMOVED = re.compile(r"^-(?!--)", re.MULTILINE)
# A lint level in Cargo.toml is a rule too, written alone or in a table with its priority.
LINT_LEVEL = re.compile(
    r'^[+-]\s*[\w:-]+\s*=\s*(\{[^}\n]*level\s*=\s*)?"(allow|warn|deny|forbid)"', re.MULTILINE
)


def git(*args: str) -> str:
    return subprocess.run(["git", *args], capture_output=True, text=True, check=True).stdout


def benched(changed: list[str]) -> dict[str, list[str]]:
    """The crates with a benchmark whose `src/` the change touches, each with its files."""
    out: dict[str, list[str]] = {}
    for f in changed:
        m = SOURCE.match(f)
        if m and (ROOT / "crates" / m[1] / "benches" / f"{m[1]}.rs").is_file():
            out.setdefault(m[1], []).append(f)
    return out


def check(message: str, changed: list[str], diff) -> list[str]:
    """What is wrong with a commit's `message`, given the files it `changed` and `diff`, a
    function giving its diff of some paths, as the hook and as the audit see one."""
    if JAPANESE.search(message):
        return ["the message must be in English (AGENTS.md writing rule 6)"]
    crates = benched(changed)
    if crates and not BENCHMARKS.search(message):
        return ["this changes the src/ of " + ", ".join(sorted(crates)) + ", so a line of the "
                "message must begin \"Benchmarks\" with their result, or be \"Benchmarks: "
                "not on a timed path\" (TESTING.md T10)"]
    rules = [f for f in changed if RULE_FILES.search(f)]
    if LINT_LEVEL.search(diff("Cargo.toml")):
        rules.append("Cargo.toml (a lint level)")
    lists = [f for f in changed if TEST_LIST.search(f)]
    if lists and REMOVED.search(diff(*lists)):
        rules.append("a test removed from a test-list.txt (TESTING.md T2)")
    if not rules:
        return []
    adrs = {m.group(1) for f in changed if (m := ADR_FILE.match(f))}
    if not adrs & set(ADR_NAME.findall(message)):
        return ["this changes a rule (" + ", ".join(rules) + "), so the message must name an "
                "ADR that is staged (AGENTS.md, \"Changing the rules\")"]
    return []


def status(added: list[str], before: str, after: str) -> Optional[str]:
    """What is wrong with a commit that ends a unit, given the files it `added` and
    ROADMAP.md `before` and `after` it: one adding an ADR or ticking a box that leaves
    ROADMAP.md §1 as it was."""
    adr = [f for f in added if ADR_FILE.match(f)]
    ticked = len(TICKED.findall(after)) > len(TICKED.findall(before))
    if not (adr or ticked):
        return None
    if (m := STATUS.search(before)) and (n := STATUS.search(after)) and m[0] != n[0]:
        return None
    why = "adds " + ", ".join(adr) if adr else "ticks a box in ROADMAP.md"
    return f"this {why}, so it ends a unit and must rewrite ROADMAP.md §1 (PLANNING.md §1)"


def shown(spec: str) -> str:
    """A file as `git show` gives it, or nothing where it is not."""
    run = subprocess.run(["git", "show", spec], capture_output=True, text=True)
    return run.stdout if run.returncode == 0 else ""


def report(crate: str, files: list[str]) -> Optional[str]:
    """What is wrong with a crate's last benchmark report, read by the hook: missing, older
    than one of the staged `files`, or an item not `ok`."""
    path = ROOT / "out" / "bench" / crate / "report.tsv"
    if not path.is_file():
        return f"{crate} has no benchmark report ({path.relative_to(ROOT)})"
    made = path.stat().st_mtime
    if any((ROOT / f).is_file() and (ROOT / f).stat().st_mtime > made for f in files):
        return f"{crate}'s benchmark report is older than its staged src/ files"
    rows = path.read_text(encoding="utf-8").splitlines()
    head = rows[0].split("\t") if rows else []
    if "verdict" not in head:
        return f"{crate}'s benchmark report has no verdict"
    at = head.index("verdict")
    failed = [r.split("\t")[0] for r in rows[1:] if r.split("\t")[at:at + 1] != ["ok"]]
    if failed:
        return f"{crate}'s benchmark report fails " + ", ".join(failed)
    return None


def kept(raw: str) -> str:
    """A message as git keeps it: comment lines aside, nothing from the scissors on."""
    raw = SCISSORS.split(raw, maxsplit=1)[0]
    return "\n".join(l for l in raw.split("\n") if not l.startswith("#"))


def staged() -> list[str]:
    """What the hook reads: the staged change. Without rename detection, a file renamed out
    of a rule's place is named where it was."""
    changed = git("diff", "--cached", "--name-only", "--no-renames").split()
    with open(sys.argv[1], encoding="utf-8") as handle:
        message = kept(handle.read())
    found = check(message, changed, lambda *p: git("diff", "--cached", "-U0", "--", *p))
    added = git("diff", "--cached", "--name-only", "--no-renames", "--diff-filter=A").split()
    if not found and (w := status(added, shown("HEAD:ROADMAP.md"), shown(":ROADMAP.md"))):
        found = [w]
    if not found and not UNTIMED.search(message):
        found = [f"{w} (TESTING.md T10): run cargo bench -p {c} --bench {c} --all-features"
                 for c, f in sorted(benched(changed).items()) if (w := report(c, f))]
    return found


def committed(commits: list[str]) -> list[str]:
    """What the audit reads: each commit made, so that one made without the hooks is
    caught by the same check after it."""
    out = []
    for c in commits:
        parent = git("rev-list", "--parents", "-n", "1", c).split()[1:2]
        base = parent[0] if parent else git("hash-object", "-t", "tree", "/dev/null").strip()
        changed = git("diff", "--name-only", "--no-renames", base, c).split()
        message = git("log", "-1", "--format=%B", c)
        found = check(message, changed, lambda *p, b=base, c=c: git("diff", "-U0", b, c, "--", *p))
        added = git("diff", "--name-only", "--no-renames", "--diff-filter=A", base, c).split()
        if not found and (w := status(added, shown(f"{base}:ROADMAP.md"), shown(f"{c}:ROADMAP.md"))):
            found = [w]
        out += [f"  {c[:12]}: {f}" for f in found]
    return out


def main() -> int:
    if sys.argv[1:2] == ["--commits"]:
        found = committed(sys.argv[2:])
        print("\n".join(found) if found else "  PASS")
        return 1 if found else 0
    found = staged()
    for f in found:
        print(f"commit-msg: {f}.", file=sys.stderr)
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
