#!/usr/bin/env python3
"""Checks a commit message against the staged change (run by `commit-msg`).

1. The message is in English (AGENTS.md writing rule 6).
2. A change to a rule carries its record (AGENTS.md, "Changing the rules"): when a staged
   file states or enforces a rule, the message names an ADR and an ADR is staged too.
"""

import re
import subprocess
import sys

JAPANESE = re.compile("[\u3040-\u30ff\u3400-\u4dbf\u4e00-\u9fff\uff66-\uff9f]")
ADR_NAME = re.compile(r"\bADR-\d{4}\b")
RULE_FILES = re.compile(
    r"^(AGENTS|PLANNING|CODING|TESTING|AUDITING)\.md$"
    r"|^(clippy|rustfmt|rust-toolchain|deny)\.toml$|^\.betterleaks\.toml$"
    r"|^scripts/(audit|hooks)/|^\.claude/settings\.json$"
)
# A lint level in Cargo.toml is a rule too.
LINT_LEVEL = re.compile(r'^[+-]\s*[\w:]+\s*=\s*"(allow|warn|deny|forbid)"', re.MULTILINE)


def git(*args: str) -> str:
    return subprocess.run(["git", *args], capture_output=True, text=True, check=True).stdout


def main() -> int:
    with open(sys.argv[1], encoding="utf-8") as handle:
        message = "\n".join(l for l in handle.read().split("\n") if not l.startswith("#"))
    if JAPANESE.search(message):
        print("commit-msg: the message must be in English (AGENTS.md writing rule 6).", file=sys.stderr)
        return 1
    staged = git("diff", "--cached", "--name-only").split()
    rules = [f for f in staged if RULE_FILES.search(f)]
    if LINT_LEVEL.search(git("diff", "--cached", "-U0", "--", "Cargo.toml")):
        rules.append("Cargo.toml (a lint level)")
    if not rules:
        return 0
    adr_staged = any(f.startswith("docs/adr/0") for f in staged)
    if not ADR_NAME.search(message) or not adr_staged:
        print(
            "commit-msg: this changes a rule (" + ", ".join(rules) + "), so the message must "
            "name an ADR and an ADR must be staged (AGENTS.md, \"Changing the rules\").",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
