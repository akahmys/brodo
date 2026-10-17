#!/usr/bin/env python3
"""Every test the gate runs is in its crate's `test-list.txt` (TESTING.md T2).

The tests of each member's library, binaries, integration tests and examples are listed by
their own executables (`--list`), ignored ones included, one line each:
`<kind> <target> <test>`. The gate fails when the list and the tests differ, either way;
`--update` rewrites the lists. A test removed from a list is a test deleted or renamed,
which the `commit-msg` hook lets through only with an ADR (`commit_msg_check.py`).
"""

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
LIST = "test-list.txt"


def executables() -> list[tuple[Path, str, str, str]]:
    out = subprocess.run(
        ["cargo", "test", "--workspace", "--lib", "--bins", "--tests", "--examples",
         "--no-run", "--quiet", "--message-format=json"],
        cwd=ROOT, capture_output=True, text=True, check=True,
    ).stdout
    found = []
    for line in out.splitlines():
        m = json.loads(line)
        if m.get("reason") == "compiler-artifact" and m.get("executable") and m["profile"]["test"]:
            crate = Path(m["manifest_path"]).parent
            found.append((crate, m["target"]["kind"][0], m["target"]["name"], m["executable"]))
    return found


def actual() -> dict[Path, list[str]]:
    lists: dict[Path, list[str]] = {}
    for crate, kind, target, exe in executables():
        listed = subprocess.run([exe, "--list", "--format", "terse"], capture_output=True,
                                text=True, check=True).stdout
        names = [l[: -len(": test")] for l in listed.splitlines() if l.endswith(": test")]
        lists.setdefault(crate, []).extend(f"{kind} {target} {n}" for n in names)
    return {c: sorted(ls) for c, ls in lists.items()}


def main() -> int:
    lists = actual()
    if "--update" in sys.argv:
        for crate, lines in lists.items():
            (crate / LIST).write_text("".join(f"{l}\n" for l in lines))
        return 0
    failures = []
    for crate, lines in lists.items():
        path = crate / LIST
        recorded = path.read_text().splitlines() if path.exists() else []
        name = path.relative_to(ROOT)
        failures += [f"  {name}: not listed: {l}" for l in sorted(set(lines) - set(recorded))]
        failures += [f"  {name}: listed, not found: {l}" for l in sorted(set(recorded) - set(lines))]
    if failures:
        print("\n".join(failures[:40]))
        print("  run 'python3 scripts/audit/tests.py --update'; a test removed needs an ADR (T2)")
        return 1
    print(f"  PASS ({sum(len(l) for l in lists.values())} tests listed)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
