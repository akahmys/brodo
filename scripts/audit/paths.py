#!/usr/bin/env python3
"""No relative path may lead outside the project (CODING.md Rule 14).

An absolute path into a home or temporary directory is betterleaks' to find, by the
`path-outside-project` rule in `.betterleaks.toml`, because the commit hooks run betterleaks
and not this. What a pattern cannot judge is a relative path: `../x` is fine in one file and
names a sibling of the project in another. This resolves each `../` path against the file
that holds it and fails when the result is outside the root.
"""

import os
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
SKIP_DIRS = {".git", "target", "out"}
SUFFIXES = {".md", ".rs", ".toml", ".py", ".sh", ".txt", ".json", ".yml", ".yaml"}
EXTRA_FILES = {"pre-commit", "commit-msg"}

RELATIVE = re.compile(r"(?<![\w.])((?:\.\./)+[^\s)\]'\"`>]*)")


def files() -> list[Path]:
    out = []
    for directory, subdirs, names in os.walk(ROOT):
        subdirs[:] = [d for d in subdirs if d not in SKIP_DIRS]
        for name in names:
            path = Path(directory) / name
            if path.suffix in SUFFIXES or name in EXTRA_FILES:
                out.append(path)
    return out


def main() -> int:
    failures = []
    for path in files():
        for number, line in enumerate(path.read_text(errors="replace").split("\n"), 1):
            for match in RELATIVE.finditer(line):
                target = Path(os.path.normpath(path.parent / match.group(1)))
                if not target.is_relative_to(ROOT):
                    failures.append(
                        f"  {path.relative_to(ROOT)}:{number}: {match.group(1)} leads outside the project"
                    )
    if failures:
        print("\n".join(failures))
        return 1
    print("  PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
