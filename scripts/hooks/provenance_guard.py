#!/usr/bin/env python3
"""Claude Code PreToolUse hook on reading tools (`.claude/settings.json`). Exit 2 blocks the call.

It stops an agent from reading what AGENTS.md S1 forbids: the sources of the solver, the design platform,
OpenVDB and OpenFOAM. It matches names, never a directory on this machine: a path or URL
with a segment, after a `/`, that starts with one of the names (`~/x/the solver/src`,
`../the solver-design.md`, `github.com/<org>/openvdb/...`) is blocked. The project sites
(`www.openvdb.org`) are hosts, not such segments, so their public documentation stays
readable.

Read: the path fields of Read, Grep and Glob (and Glob's pattern), WebFetch's URL, and
Bash's command. A name without a `/` before it (`cd ..; ls the solver`) passes; the rule binds
regardless.
"""

import json
import re
import sys

FORBIDDEN = re.compile(r"/(the solver|the design platform|openvdb|openfoam)[^/\s'\"]*", re.IGNORECASE)
FIELDS = ("file_path", "path", "url", "command", "notebook_path")


def main() -> int:
    call = json.load(sys.stdin)
    tool_input = call.get("tool_input", {})
    values = [tool_input.get(f, "") for f in FIELDS]
    if call.get("tool_name") == "Glob":
        values.append(tool_input.get("pattern", ""))
    for value in values:
        match = FORBIDDEN.search(value if isinstance(value, str) else "")
        if match:
            print(f"Blocked by scripts/hooks/provenance_guard.py: {match.group(0)} is a source "
                  "brodo may not read (AGENTS.md S1). Ask the human.", file=sys.stderr)
            return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
