#!/usr/bin/env python3
"""Every test the gate runs is in its crate's `test-list.txt` (TESTING.md T2).

The tests of each member's library, binaries, integration tests and examples are listed by
their own executables (`--list`), ignored ones included, one line each:
`<kind> <target> <test>`; its doctests, a README's code and a `compile_fail` example among
them, by `cargo test --doc -- --list`, as `doc <crate> <file> - <item> #<k>`, the k-th of
that item's, without the line rustdoc names, so that an edit above one does not rename it.
A crate with a list and no test is held to an empty one. The gate fails when the list and
the tests differ, either way; `--update` rewrites the lists. A test removed from a list is a test deleted or renamed,
which the `commit-msg` hook lets through only with an ADR (`commit_msg_check.py`).
"""

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
LIST = "test-list.txt"


def executables() -> list[tuple[Path, str, str, str]]:
    out = subprocess.run(
        ["cargo", "test", "--workspace", "--all-features", "--lib", "--bins", "--tests", "--examples",
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


def doctests(crate: Path) -> list[str]:
    """A crate's doctests, each by its file, its item and its place among the item's."""
    listed = subprocess.run(
        ["cargo", "test", "-p", crate.name, "--doc", "--all-features", "--", "--list",
         "--format", "terse"],
        cwd=ROOT, capture_output=True, text=True, check=True,
    ).stdout
    out, seen = [], {}
    for line in listed.splitlines():
        if not line.endswith(": test"):
            continue
        name = re.sub(r" \(line \d+\)", "", line[: -len(": test")])
        seen[name] = seen.get(name, 0) + 1
        out.append(f"doc {crate.name} {name} #{seen[name]}")
    return out


def actual() -> dict[Path, list[str]]:
    lists: dict[Path, list[str]] = {p.parent: [] for p in (ROOT / "crates").glob(f"*/{LIST}")}
    for crate, kind, target, exe in executables():
        listed = subprocess.run([exe, "--list", "--format", "terse"], capture_output=True,
                                text=True, check=True).stdout
        names = [l[: -len(": test")] for l in listed.splitlines() if l.endswith(": test")]
        lists.setdefault(crate, []).extend(f"{kind} {target} {n}" for n in names)
    for crate in list(lists):
        if (crate / "src" / "lib.rs").exists():
            lists[crate].extend(doctests(crate))
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
