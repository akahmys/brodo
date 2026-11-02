#!/usr/bin/env python3
"""Checks a commit message against the staged change (run by `commit-msg`).

1. The message is in English (AGENTS.md writing rule 6).
2. A change to a rule carries its record (AGENTS.md, "Changing the rules"): when a staged
   file states or enforces a rule, the message names an ADR that is staged too.
3. A test deleted or renamed carries its record (TESTING.md T2): when a line leaves a
   crate's `test-list.txt`, the message names an ADR that is staged too.

The message is read as git keeps it: comment lines aside, and nothing from the scissors
line on, below which `git commit -v` shows the staged diff.
"""

import re
import subprocess
import sys

# Japanese letters, and CJK punctuation and full-width forms.
JAPANESE = re.compile("[\u3000-\u30ff\u3400-\u4dbf\u4e00-\u9fff\uff00-\uffef]")
ADR_NAME = re.compile(r"\bADR-(\d{4})\b")
ADR_FILE = re.compile(r"^docs/adr/(\d{4})-")
SCISSORS = re.compile(r"^# -+ >8 -+$", re.MULTILINE)
RULE_FILES = re.compile(
    r"^(AGENTS|PLANNING|CODING|TESTING|AUDITING)\.md$"
    r"|^(clippy|rustfmt|rust-toolchain|deny)\.toml$|^\.betterleaks\.toml$"
    r"|^scripts/(audit|hooks)/|^\.claude/settings\.json$|^\.github/workflows/"
    r"|^\.cargo/mutants\.toml$"
)
TEST_LIST = re.compile(r"^crates/[^/]+/test-list\.txt$")
REMOVED = re.compile(r"^-(?!--)", re.MULTILINE)
# A lint level in Cargo.toml is a rule too, written alone or in a table with its priority.
LINT_LEVEL = re.compile(
    r'^[+-]\s*[\w:-]+\s*=\s*(\{[^}\n]*level\s*=\s*)?"(allow|warn|deny|forbid)"', re.MULTILINE
)


def git(*args: str) -> str:
    return subprocess.run(["git", *args], capture_output=True, text=True, check=True).stdout


def main() -> int:
    with open(sys.argv[1], encoding="utf-8") as handle:
        raw = SCISSORS.split(handle.read(), maxsplit=1)[0]
    message = "\n".join(l for l in raw.split("\n") if not l.startswith("#"))
    if JAPANESE.search(message):
        print("commit-msg: the message must be in English (AGENTS.md writing rule 6).", file=sys.stderr)
        return 1
    # Without rename detection, a file renamed out of a rule's place is named where it was.
    staged = git("diff", "--cached", "--name-only", "--no-renames").split()
    rules = [f for f in staged if RULE_FILES.search(f)]
    if LINT_LEVEL.search(git("diff", "--cached", "-U0", "--", "Cargo.toml")):
        rules.append("Cargo.toml (a lint level)")
    lists = [f for f in staged if TEST_LIST.search(f)]
    if lists and REMOVED.search(git("diff", "--cached", "-U0", "--", *lists)):
        rules.append("a test removed from a test-list.txt (TESTING.md T2)")
    if not rules:
        return 0
    staged_adrs = {m.group(1) for f in staged if (m := ADR_FILE.match(f))}
    if not staged_adrs & set(ADR_NAME.findall(message)):
        print(
            "commit-msg: this changes a rule (" + ", ".join(rules) + "), so the message must "
            "name an ADR that is staged (AGENTS.md, \"Changing the rules\").",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
