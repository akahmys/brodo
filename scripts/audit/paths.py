#!/usr/bin/env python3
"""No relative path may lead outside the project (CODING.md Rule 14).

An absolute path into a home or temporary directory is betterleaks' to find, by the
`path-outside-project` rule in `.betterleaks.toml`, because the commit hooks run betterleaks
and not this. What a pattern cannot judge is a relative path: `../x` is fine in one file and
names a sibling of the project in another. This resolves each `../` path against the file
that holds it and fails when the result is outside the root.

It also holds CODING.md L2 to one list, `OUTPUT_DIRS` in `project.py`: each directory there
is git-ignored at the root and in `.betterleaks.toml`'s path allow-list at the root alone.
This scan and `documents.py` read the files git would commit, tracked or not ignored, so
that a directory of an output's name deeper in the tree is read.
"""

import os
import re
import subprocess
import sys
from pathlib import Path

from project import OUTPUT_DIRS

ROOT = Path(__file__).resolve().parent.parent.parent
SUFFIXES = {".md", ".rs", ".toml", ".py", ".sh", ".txt", ".json", ".yml", ".yaml"}
EXTRA_FILES = {"pre-commit", "commit-msg"}

RELATIVE = re.compile(r"(?<![\w.])((?:\.\./)+[^\s)\]'\"`>]*)")


def committable() -> list[Path]:
    """The files git would commit: tracked, or untracked and not ignored."""
    listed = subprocess.run(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard"],
        cwd=ROOT, capture_output=True, text=True, check=True,
    ).stdout.splitlines()
    return [ROOT / f for f in listed if (ROOT / f).is_file()]


def files() -> list[Path]:
    return [p for p in committable() if p.suffix in SUFFIXES or p.name in EXTRA_FILES]


def output_dirs() -> list[str]:
    """Each output directory missing from `.gitignore` or betterleaks' allow-list (L2)."""
    ignored = (ROOT / ".gitignore").read_text().split("\n")
    leaks = (ROOT / ".betterleaks.toml").read_text()
    allow = leaks[leaks.find("[[allowlists]]") :] if "[[allowlists]]" in leaks else ""
    allow = allow[: allow.find("\n]") + 2]
    failures = []
    for d in OUTPUT_DIRS:
        if f"/{d}/" not in ignored:
            failures.append(f"  .gitignore: /{d}/ is not ignored (CODING.md L2)")
        if f"'''^{d}/'''" not in allow:
            failures.append(f"  .betterleaks.toml: {d}/ is not in the path allow-list (CODING.md L2)")
    return failures


def main() -> int:
    failures = output_dirs()
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
