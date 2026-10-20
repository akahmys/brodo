#!/usr/bin/env python3
"""Claude Code PreToolUse hook on reading tools (`.claude/settings.json`). Exit 2 blocks the call.

It stops an agent from reading what AGENTS.md S1 forbids: the sources of OpenVDB and
OpenFOAM, and of the projects in the owner's local list, one name a line, `#` starting a
comment, at `$BRODO_FORBIDDEN_SOURCES` or `~/.config/brodo/forbidden-sources`. The list
stays outside the repository, which names no other project of its owner's (ADR-0007). It
matches names, never a directory on this machine: a path or URL with a segment, after a
`/`, that starts with one of the names (`~/x/<name>/src`, `../<name>-design.md`,
`github.com/<org>/<name>/...`) is blocked. The project sites (`www.openvdb.org`) are
hosts, not such segments, so their public documentation stays readable.

Read: the path fields of Read, Grep and Glob (and Glob's pattern), WebFetch's URL, and
Bash's command. A name without a `/` before it passes; the rule binds regardless.

With `--check`, the audit's: fails unless the local list exists and names a project.
"""

import json
import os
import re
import sys
from pathlib import Path

BUILT_IN = ("openvdb", "openfoam")
FIELDS = ("file_path", "path", "url", "command", "notebook_path")


def local_list() -> Path:
    named = os.environ.get("BRODO_FORBIDDEN_SOURCES")
    return Path(named) if named else Path.home() / ".config" / "brodo" / "forbidden-sources"


def local_names() -> list[str]:
    try:
        lines = local_list().read_text().splitlines()
    except OSError:
        return []
    names = (line.split("#", 1)[0].strip() for line in lines)
    return [n for n in names if n]


def forbidden() -> re.Pattern[str]:
    names = "|".join(re.escape(n) for n in (*BUILT_IN, *local_names()))
    return re.compile(rf"/({names})[^/\s'\"]*", re.IGNORECASE)


def check() -> int:
    names = local_names()
    if names:
        print(f"  PASS ({len(names)} names in the local list)")
        return 0
    print(f"  FAIL: no names in the local list of forbidden sources at {local_list()}; "
          "set $BRODO_FORBIDDEN_SOURCES or write the default (AGENTS.md S1)")
    return 1


def main() -> int:
    if sys.argv[1:] == ["--check"]:
        return check()
    pattern = forbidden()
    call = json.load(sys.stdin)
    tool_input = call.get("tool_input", {})
    values = [tool_input.get(f, "") for f in FIELDS]
    if call.get("tool_name") == "Glob":
        values.append(tool_input.get("pattern", ""))
    for value in values:
        match = pattern.search(value if isinstance(value, str) else "")
        if match:
            print(f"Blocked by scripts/hooks/provenance_guard.py: {match.group(0)} is a source "
                  "brodo may not read (AGENTS.md S1). Ask the human.", file=sys.stderr)
            return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
