#!/usr/bin/env python3
"""Claude Code PreToolUse hook on Bash (`.claude/settings.json`). Exit 2 blocks the call.

It stops what would get round the commit hooks or leave the machine (AUDITING.md §2):
`--no-verify` or `-n` on a commit, any change to `core.hooksPath` other than installing
the hooks, and every `git push` but one with no option to `origin` of `main` and version
tags `vX.Y.Z` (ADR-0003).

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
]
PUSH = re.compile(r"^(\s+(-C\s+\S+|-c\s+\S+|--\S+))*\s+push\b(.*)$", re.DOTALL)
# What may be pushed: the main branch and version tags, by name, to origin.
PUSHABLE = re.compile(r"^(main|v\d+\.\d+\.\d+)$")


def push_allowed(rest: str) -> bool:
    """Whether the words after `push` are `origin` and one or more pushable refs, with no
    option, no `+` forcing a ref and no `:` naming another."""
    words = rest.split()
    return len(words) >= 2 and words[0] == "origin" and all(PUSHABLE.match(w) for w in words[1:])


HEREDOC = re.compile(r"<<-?\s*(['\"]?)(\w+)\1[^\n]*\n.*?\n\s*\2\s*(?=\n|$)", re.DOTALL)
QUOTED = re.compile(r"'[^']*'|\"(?:\\.|[^\"\\])*\"")
# A redirection with its target, `2>&1`, `> out.log`, `&> all.log`: its `&` does not end a
# command, and its words are not the command's (ADR-0003).
REDIRECT = re.compile(r"(?:\d*|&)(?:>>|>|<)&?\s*[^\s;&|<>]+")


def code_only(command: str) -> str:
    """The command without heredoc bodies, quoted strings and redirections, which are data,
    not commands."""
    return REDIRECT.sub(" ", QUOTED.sub("''", HEREDOC.sub("", command)))


def main() -> int:
    command = json.load(sys.stdin).get("tool_input", {}).get("command", "")
    for segment in SEGMENT.findall(code_only(command)):
        invocation = GIT.match(segment)
        if not invocation:
            continue
        found = [what for pattern, what in BLOCKED if pattern.search(invocation.group(1))]
        push = PUSH.match(invocation.group(1))
        if push and not push_allowed(push.group(3)):
            found.append("git push other than origin's main and version tags, with no option")
        if found:
            print(f"Blocked by scripts/hooks/claude_guard.py: {found[0]}. Ask the human.",
                  file=sys.stderr)
            return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
