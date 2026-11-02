#!/usr/bin/env python3
"""Claude Code PreToolUse hook on every tool (`.claude/settings.json`); a call naming a
shell `command` is read. Exit 2 blocks the call.

It stops, as a convenience, what would get round the commit hooks or leave the machine
(AUDITING.md §2): `--no-verify`, any abbreviation of it, or `-n` on a commit; any git command
naming `core.hooksPath` but installing the hooks or reading the setting; git's configuration
changed for one call, by `-c`, `GIT_CONFIG_*`, `HOME` or `XDG_CONFIG_HOME`; an alias, an
include or a remote's URL set in git's configuration; a commit or a ref made by plumbing,
which runs no hook; and every `git push` but one with no option, before or after `push` but
`-C`, to `origin` of `main` and version tags `vX.Y.Z` (ADR-0003).

The command is split into words as the shell splits them (`shlex`), quotes and escapes
removed, and into commands at `;`, `&`, `|`, parentheses, braces, backticks and line ends,
heredoc bodies aside. In each command, a word `git`, or a path ending in it, starts a git
invocation wherever it stands, after keywords, assignments or any command that runs
another; a quoted string holding one is a word of its own and is not. A command written
another way (`sh -c`, a variable) can pass: the backstop is the audit, which holds every
commit made, on the owner's machine and on GitHub, to the checks the hooks run.
"""

import json
import re
import shlex
import sys

# What the guard refuses, by its message.
SKIPPING = "skipping the commit hooks"
HOOKS = "changing core.hooksPath"
CONFIG = "changing git's configuration for one call or for good"
PLUMBING = "making a commit or a ref by plumbing, which runs no hook"
PUSH = "git push other than origin's main and version tags, with no option"

# The commands whose hooks `--no-verify` skips.
HOOKED = {"commit", "merge", "rebase", "cherry-pick", "am", "revert"}
# Plumbing that makes a commit or moves a ref with no hook run.
PLUMB = {"commit-tree", "update-ref", "mktree", "replace"}
# Settings that change where configuration, hooks or a push go.
SETTING = re.compile(r"^(core\.hookspath|alias\.|include\.|includeif\.|remote\.[^=]*\.(push)?url)",
                     re.IGNORECASE)
# Variables that make git read another configuration.
ENVIRONMENT = re.compile(r"^(GIT_CONFIG\w*|HOME|XDG_CONFIG_HOME)=")
# git's options before its command that take the next word as their argument.
OPTION_ARGUMENT = {"-C", "-c", "--git-dir", "--work-tree", "--namespace", "--super-prefix",
                   "--config-env", "--exec-path", "--list-cmds", "--attr-source"}
# What may be pushed: the main branch and version tags, by name, to origin.
PUSHABLE = re.compile(r"^(main|v\d+\.\d+\.\d+)$")
# A heredoc's body, from the line after its operator to its delimiter; the rest of the
# operator's line is a command's and is kept (group 3).
HEREDOC = re.compile(r"<<-?\s*(['\"]?)(\w+)\1([^\n]*)\n(?:.*?\n)?\s*\2\s*(?=\n|$)", re.DOTALL)
# The words that end one command and start another.
SEPARATORS = {";", "&", "&&", "|", "||", "(", ")", "{", "}", "\n", ";;", "|&", "`"}


def commands(command: str) -> list[list[str]]:
    """The command's commands, each its words, quotes and escapes removed."""
    text = HEREDOC.sub(r"\3", command).replace("`", " ` ").replace("$(", " ( ")
    lexer = shlex.shlex(text, posix=True, punctuation_chars=";&|(){}")
    lexer.whitespace = " \t\r"
    lexer.whitespace_split = True
    try:
        words = list(lexer)
    except ValueError:
        # An unclosed quote: read word by word, as the shell would not.
        words = text.split()
    out, current = [], []
    for w in words:
        if w in SEPARATORS or set(w) <= set(";&|(){}\n"):
            if current:
                out.append(current)
            current = []
        elif "\n" in w:
            parts = w.split("\n")
            current.append(parts[0])
            out.append(current)
            current = [p for p in parts[1:] if p]
        else:
            current.append(w)
    if current:
        out.append(current)
    return out


def split_options(words: list[str]) -> tuple[list[list[str]], list[str]]:
    """git's options before its command, each with its argument, and the words from the
    command on."""
    options, i = [], 0
    while i < len(words) and words[i].startswith("-"):
        takes = words[i] in OPTION_ARGUMENT
        options.append(words[i : i + 1 + takes])
        i += 1 + takes
    return options, words[i:]


def push_allowed(words: list[str]) -> bool:
    """Whether the words after `push` are `origin` and one or more pushable refs, with no
    option, no `+` forcing a ref and no `:` naming another."""
    return len(words) >= 2 and words[0] == "origin" and all(PUSHABLE.match(w) for w in words[1:])


def skips_hooks(sub: str, args: list[str]) -> bool:
    """`--no-verify`, abbreviated to `--no-v` or more, or a short option cluster holding `n`."""
    return sub in HOOKED and any(
        re.fullmatch(r"--no-v[a-z]*(=.*)?", a) or re.fullmatch(r"-[a-zA-Z]*n[a-zA-Z]*", a)
        for a in args
    )


def git_call(before: list[str], after: list[str]) -> list[str]:
    """What one git invocation does that the guard stops: `before` the words ahead of it in
    its command, `after` its own."""
    found = []
    if any(ENVIRONMENT.match(w) for w in before):
        found.append(CONFIG)
    options, rest = split_options(after)
    sub, args = (rest[0], rest[1:]) if rest else ("", [])
    settings = [o[1] for o in options if o[0] == "-c" and len(o) > 1]
    if any(re.match(r"core\.hookspath", s, re.IGNORECASE) for s in settings):
        found.append(HOOKS)
    elif any(SETTING.match(s) for s in settings) or any(o[0] == "--config-env" for o in options):
        found.append(CONFIG)
    if skips_hooks(sub, args):
        found.append(SKIPPING)
    if sub == "config":
        named = [a for a in args if SETTING.match(a)]
        hooks = [a for a in named if a.lower().startswith("core.hookspath")]
        install = args in (["core.hooksPath", "scripts/hooks"], ["--local", "core.hooksPath",
                           "scripts/hooks"], ["--get", "core.hooksPath"])
        if hooks and not install:
            found.append(HOOKS)
        elif named and not hooks:
            found.append(CONFIG)
    if sub in PLUMB:
        found.append(PLUMBING)
    if sub == "push" and not (all(o[0] == "-C" for o in options) and push_allowed(args)):
        found.append(PUSH)
    return found


def blocked(command: str) -> list[str]:
    """What the command does that the guard stops."""
    found = []
    if re.search(r"\bGIT_CONFIG_\w*\s*=", command):
        found.append(CONFIG)
    for words in commands(command):
        for i, w in enumerate(words):
            if w == "git" or w.endswith("/git"):
                found += git_call(words[:i], words[i + 1 :])
                break
    return list(dict.fromkeys(found))


def main() -> int:
    # Every tool's call comes here (.claude/settings.json); one that runs a shell command
    # names it `command`, and any other has none to read.
    command = json.load(sys.stdin).get("tool_input", {}).get("command", "")
    if not isinstance(command, str):
        return 0
    found = blocked(command)
    if found:
        print(f"Blocked by scripts/hooks/claude_guard.py: {found[0]}. Ask the human.",
              file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
