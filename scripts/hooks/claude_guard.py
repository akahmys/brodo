#!/usr/bin/env python3
"""Claude Code PreToolUse hook on Bash (`.claude/settings.json`). Exit 2 blocks the call.

It stops what would get round the commit hooks or leave the machine (AUDITING.md §2):
`--no-verify`, any abbreviation of it, or `-n` on a commit, quoted or not; any git command
naming `core.hooksPath` but installing the hooks or reading the setting, `git -c`, `--unset`
and `GIT_CONFIG_*` variables included; an alias made in git's configuration; and every
`git push` but one with no option, before or after `push` but `-C`, to `origin` of `main`
and version tags `vX.Y.Z` (ADR-0003).

Only a git invocation is read: once heredoc bodies, redirections and quoted strings holding
white space are removed, and a quoted word unquoted, a segment of the command, split at `;`,
`&`, `|`, parentheses, braces, backticks and line ends, whose first word, after shell
keywords such as `if` and `then`, commands that run another, `env`, `sudo` or `command`,
with their options, and variable assignments, is `git` or a path ending in it. Text that
merely mentions git is not one. `core.hooksPath`, an alias and `GIT_CONFIG_*` are also
looked for in quoted strings, but not in heredoc bodies; the rest of a heredoc operator's
line is read. A command written another way (`sh -c`, a variable) can pass; the audit's
history scan is the backstop.
"""

import json
import re
import sys

SEGMENT = re.compile(r"[^;&|\n(){}`]+")
# Words a shell lets stand before a command.
# A command that runs another, `env`, `sudo` or `command`, may carry options, an option's
# argument among them.
LEAD = (r"(?:(?:if|then|else|elif|do|while|until|!)\s+"
        r"|(?:time|exec|command|nohup|env|sudo|xargs)(?:\s+-\S+(?:\s+[^-\s]\S*)?)*\s+)*")
# `git`, or a path ending in it.
GIT = re.compile(rf"^\s*{LEAD}(?:\w+=\S*\s+)*(?:\S*/)?git(?=\s|$)(.*)$", re.DOTALL)
# Abbreviations git accepts for --no-verify: --no-v and longer.
SKIP = re.compile(r"\b(commit|merge|rebase|cherry-pick|am|revert)\b.*"
                  r"(--no-v[a-z]*\b|\s-[a-zA-Z]*n[a-zA-Z]*\b)")
# Read with quoted strings kept: a setting can be quoted.
HOOKS_PATH = re.compile(r"core\.hookspath", re.IGNORECASE)
# Installing the hooks, or reading where they are: all a command naming `core.hooksPath` may do.
HOOKS_PATH_ALLOWED = re.compile(
    r"\s*config\s+(?:--local\s+)?(?:core\.hookspath\s+scripts/hooks|--get\s+core\.hookspath)\s*",
    re.IGNORECASE,
)
ALIAS = re.compile(r"\balias\.", re.IGNORECASE)
ENVIRONMENT = re.compile(r"\bGIT_CONFIG_(?:COUNT|KEY_\d+|VALUE_\d+|PARAMETERS|GLOBAL|SYSTEM)\s*=")
# git's options before its command that take the next word as their argument.
OPTION_ARGUMENT = {"-C", "-c", "--git-dir", "--work-tree", "--namespace", "--super-prefix",
                   "--config-env", "--exec-path", "--list-cmds", "--attr-source"}
# What may be pushed: the main branch and version tags, by name, to origin.
PUSHABLE = re.compile(r"^(main|v\d+\.\d+\.\d+)$")


def split_options(words: list[str]) -> tuple[list[list[str]], list[str]]:
    """git's options before its command, each with its argument, and the words from the
    command on."""
    options, i = [], 0
    while i < len(words) and words[i].startswith("-"):
        takes = words[i] in OPTION_ARGUMENT
        options.append(words[i : i + 1 + takes])
        i += 1 + takes
    return options, words[i:]


def push_allowed(rest: str) -> bool:
    """Whether the words after `push` are `origin` and one or more pushable refs, with no
    option, no `+` forcing a ref and no `:` naming another."""
    words = rest.split()
    return len(words) >= 2 and words[0] == "origin" and all(PUSHABLE.match(w) for w in words[1:])


# A heredoc's body, from the line after its operator to its delimiter; the rest of the
# operator's line is a command's and is kept (group 3).
HEREDOC = re.compile(r"<<-?\s*(['\"]?)(\w+)\1([^\n]*)\n(?:.*?\n)?\s*\2\s*(?=\n|$)", re.DOTALL)
QUOTED = re.compile(r"'[^']*'|\"(?:\\.|[^\"\\])*\"")
# A redirection with its target, `2>&1`, `> out.log`, `&> all.log`: its `&` does not end a
# command, and its words are not the command's (ADR-0003).
REDIRECT = re.compile(r"(?:\d*|&)(?:>>|>|<)&?\s*[^\s;&|<>]+")


def word(m: re.Match[str]) -> str:
    """A quoted string as the shell passes it on: one word, its quotes gone, if it holds no
    white space; data, emptied, if it does."""
    inner = m.group(0)[1:-1]
    return inner if inner and not re.search(r"\s|[;&|(){}`]", inner) else "''"


def code_only(command: str) -> str:
    """The command without heredoc bodies, redirections and quoted strings holding white
    space, which are data, not commands; a quoted word, `"--no-verify"`, is kept unquoted."""
    return REDIRECT.sub(" ", QUOTED.sub(word, HEREDOC.sub(r"\3", command)))


def unquoted(command: str) -> str:
    """The command without heredoc bodies, each quoted string's text kept, its separators
    made spaces, so that a quoted setting stays in its segment."""
    def keep(m: re.Match[str]) -> str:
        return re.sub(r"[;&|\n(){}`]", " ", m.group(0)[1:-1])

    return QUOTED.sub(keep, HEREDOC.sub(r"\3", command))


def blocked(command: str) -> list[str]:
    """What the command does that the guard stops."""
    found = []
    if ENVIRONMENT.search(unquoted(command)):
        found.append("setting git's configuration through GIT_CONFIG_* variables")
    for segment in SEGMENT.findall(unquoted(command)):
        invocation = GIT.match(segment)
        if not invocation:
            continue
        rest = invocation.group(1)
        if HOOKS_PATH.search(rest) and not HOOKS_PATH_ALLOWED.fullmatch(rest):
            found.append("changing core.hooksPath")
        if ALIAS.search(rest):
            found.append("a git alias")
    for segment in SEGMENT.findall(code_only(command)):
        invocation = GIT.match(segment)
        if not invocation:
            continue
        if SKIP.search(invocation.group(1)):
            found.append("skipping the commit hooks")
        options, command_words = split_options(invocation.group(1).split())
        if command_words[:1] == ["push"] and not (
            all(o[0] == "-C" for o in options) and push_allowed(" ".join(command_words[1:]))
        ):
            found.append("git push other than origin's main and version tags, with no option")
    return found


def main() -> int:
    command = json.load(sys.stdin).get("tool_input", {}).get("command", "")
    found = blocked(command)
    if found:
        print(f"Blocked by scripts/hooks/claude_guard.py: {found[0]}. Ask the human.",
              file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
