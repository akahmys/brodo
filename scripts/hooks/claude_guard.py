#!/usr/bin/env python3
"""Claude Code PreToolUse hook on Bash (`.claude/settings.json`). Exit 2 blocks the call.

It stops what would get round the commit hooks or leave the machine (AUDITING.md §2):
`--no-verify` or `-n` on a commit, any change to `core.hooksPath` other than installing
the hooks, and `git push`, until the human decides the repository has a remote.

Only a git invocation is read: once heredoc bodies and quoted strings are removed, a
segment of the command, split at `;`, `&`, `|` and line ends, whose first word is `git`.
Text that merely mentions git is not one. A command written another way (`sh -c`, a variable) can pass; the audit's history
scan is the backstop.
"""

import json
import re
import sys

SEGMENT = re.compile(r"[^;&|\n]+")
GIT = re.compile(r"^\s*(?:\w+=\S*\s+)*git\b(.*)$", re.DOTALL)
BLOCKED = [
    (re.compile(r"\b(commit|merge|rebase|cherry-pick|am|revert)\b.*"
                r"(--no-verify\b|\s-[a-zA-Z]*n[a-zA-Z]*\b)"),
     "skipping the commit hooks"),
    (re.compile(r"core\.hookspath(?!\s+scripts/hooks\s*$)", re.IGNORECASE),
     "changing core.hooksPath"),
    (re.compile(r"^(\s+(-C\s+\S+|-c\s+\S+|--\S+))*\s+push\b"),
     "git push: pushing is the human's to do"),
]


HEREDOC = re.compile(r"<<-?\s*(['\"]?)(\w+)\1[^\n]*\n.*?\n\s*\2\s*(?=\n|$)", re.DOTALL)
QUOTED = re.compile(r"'[^']*'|\"(?:\\.|[^\"\\])*\"")


def code_only(command: str) -> str:
    """The command without heredoc bodies and quoted strings, which are data, not commands."""
    return QUOTED.sub("''", HEREDOC.sub("", command))


def main() -> int:
    command = json.load(sys.stdin).get("tool_input", {}).get("command", "")
    for segment in SEGMENT.findall(code_only(command)):
        invocation = GIT.match(segment)
        if not invocation:
            continue
        for pattern, what in BLOCKED:
            if pattern.search(invocation.group(1)):
                print(f"Blocked by scripts/hooks/claude_guard.py: {what}. Ask the human.",
                      file=sys.stderr)
                return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
