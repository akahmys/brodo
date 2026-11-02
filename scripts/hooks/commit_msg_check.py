#!/usr/bin/env python3
"""Checks a commit message against the staged change (run by `commit-msg`).

1. The message is in English (AGENTS.md writing rule 6).
2. A change to a rule carries its record (AGENTS.md, "Changing the rules"): when a staged
   file states or enforces a rule, the message names an ADR that is staged too.
3. A test deleted or renamed carries its record (TESTING.md T2): when a line leaves a
   crate's `test-list.txt`, the message names an ADR that is staged too.

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

# The audit's own lists, so that a commit is held to the same as the repository.
sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "audit"))
from paths import JAPANESE  # noqa: E402
from project import RULE_FILES as RULE_PATTERN  # noqa: E402

ADR_NAME = re.compile(r"\bADR-(\d{4})\b")
ADR_FILE = re.compile(r"^docs/adr/(\d{4})-")
SCISSORS = re.compile(r"^# -+ >8 -+$", re.MULTILINE)
RULE_FILES = re.compile(RULE_PATTERN)
TEST_LIST = re.compile(r"^crates/[^/]+/test-list\.txt$")
REMOVED = re.compile(r"^-(?!--)", re.MULTILINE)
# A lint level in Cargo.toml is a rule too, written alone or in a table with its priority.
LINT_LEVEL = re.compile(
    r'^[+-]\s*[\w:-]+\s*=\s*(\{[^}\n]*level\s*=\s*)?"(allow|warn|deny|forbid)"', re.MULTILINE
)


def git(*args: str) -> str:
    return subprocess.run(["git", *args], capture_output=True, text=True, check=True).stdout


def check(message: str, changed: list[str], diff) -> list[str]:
    """What is wrong with a commit's `message`, given the files it `changed` and `diff`, a
    function giving its diff of some paths, as the hook and as the audit see one."""
    if JAPANESE.search(message):
        return ["the message must be in English (AGENTS.md writing rule 6)"]
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
    return check(message, changed, lambda *p: git("diff", "--cached", "-U0", "--", *p))


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
