#!/usr/bin/env python3
"""Reads `cargo clippy --message-format=json` on stdin and reports the named lints only.

`python3 lint_filter.py clippy::unwrap_used clippy::panic < clippy.json` prints each
finding of those lints with its location and exits 1 if there is one. The workspace lints
fire in the same run, and without this filter a step would report them as its own.

`--under <dir>/` keeps only findings in files under that directory. `cargo clippy -p`
also lints the workspace crates the package depends on, so a lint meant for one crate
needs it.
"""

import json
import sys


def main() -> int:
    args = sys.argv[1:]
    under = ""
    if args[:1] == ["--under"]:
        under, args = args[1], args[2:]
    wanted = set(args)
    found = set()
    for line in sys.stdin:
        try:
            item = json.loads(line)
        except json.JSONDecodeError:
            continue
        message = item.get("message") or {}
        code = (message.get("code") or {}).get("code")
        if item.get("reason") != "compiler-message" or code not in wanted:
            continue
        span = next((s for s in message.get("spans", []) if s.get("is_primary")), None)
        if under and not (span and span["file_name"].startswith(under)):
            continue
        where = f"{span['file_name']}:{span['line_start']}" if span else "?"
        found.add(f"  {where}: {message.get('message')} ({code})")
    print("\n".join(sorted(found)) if found else "  PASS")
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
