#!/usr/bin/env python3
"""A release from 1.0 breaks no promise its version does not allow (TESTING.md V9; ADR-0011).

The tool's version is the one `.github/workflows/audit.yml` installs, which the gate's
`[Tools]` step holds the running one to.

1. The baseline is the newest release tag, `vMAJOR.MINOR.PATCH`, merged into HEAD. The
   check runs only while the workspace version and the baseline are both 1.0 or above and
   of one MAJOR, and CHANGELOG.md's `## Unreleased` has no `### Breaking`: a break
   declared there is the decision that the next version raises MAJOR. Otherwise it says
   why it is skipped.
2. Each member's library is compared with the baseline's by `cargo semver-checks`, with no
   feature, each feature the baseline has alone, and every feature, so that an item moved
   behind a feature is seen; a version equal to the baseline's is judged a MINOR.
3. `mestolo`'s command line and output, which no rustdoc shows, are held by the baseline's
   `crates/mestolo/tests/`, run against the current code in a copy of the tree under
   `target/semver/`, but for those `SET_ASIDE` names for the baseline's tag: tests that
   held as refused or left out what a MINOR now does. An entry of another tag, or naming
   no test of its tag, fails the check, so none outlives its baseline.
4. A version whose CHANGELOG.md section has `### Breaking` raises MAJOR over the baseline,
   from a baseline of 1.0 on.

What the tool does not see, a type changed, a bound or a lifetime narrowed, a constant's
value, a behaviour, is left to review (ADR-0011).
"""

import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

from paths import committable

ROOT = Path(__file__).resolve().parent.parent.parent
TAG = re.compile(r"^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")
WORKSPACE_VERSION = re.compile(r'^version\s*=\s*"([^"]+)"', re.MULTILINE)
FEATURES = re.compile(r"^\[features\]\s*$(.*?)(?=^\[|\Z)", re.MULTILINE | re.DOTALL)
FEATURE = re.compile(r"^([\w-]+)\s*=", re.MULTILINE)
SECTION = re.compile(r"^## (.+?)$(.*?)(?=^## |\Z)", re.MULTILINE | re.DOTALL)
BREAKING = re.compile(r"^### Breaking\s*$", re.MULTILINE)
COPY = ROOT / "target" / "semver" / "release-tests"

# The baseline's mestolo tests not run, by tag and test name, each with why (ADR-0011).
SET_ASIDE: dict[tuple[str, str], str] = {}


def run(args: list[str], cwd: Path = ROOT) -> subprocess.CompletedProcess:
    return subprocess.run(args, cwd=cwd, capture_output=True, text=True)


def version_of(text: str) -> tuple:
    m = WORKSPACE_VERSION.search(text)
    parts = m.group(1).split(".") if m else []
    return tuple(int(p) for p in parts) if len(parts) == 3 and all(p.isdigit() for p in parts) else ()


def baseline() -> tuple:
    """The newest release tag merged into HEAD, and its version; none, ("", ())."""
    tags = run(["git", "tag", "--merged", "HEAD", "--list", "v*"]).stdout.split()
    versions = [(tuple(int(g) for g in m.groups()), t) for t in tags if (m := TAG.match(t))]
    if not versions:
        return "", ()
    version, tag = max(versions)
    return tag, version


def sections() -> dict:
    return {m.group(1).split(" ")[0]: m.group(2) for m in SECTION.finditer((ROOT / "CHANGELOG.md").read_text())}


def features(text: str) -> list[str]:
    m = FEATURES.search(text)
    return [f for f in FEATURE.findall(m.group(1)) if f != "default"] if m else []


def members() -> list[dict]:
    meta = run(["cargo", "metadata", "--no-deps", "--format-version", "1"])
    return json.loads(meta.stdout)["packages"] if meta.returncode == 0 else []


def library(pkg: dict, tag: str) -> list[str]:
    """`cargo semver-checks` on one member, with no feature, each alone, and every one."""
    if not any("lib" in t["kind"] for t in pkg["targets"]):
        return []
    name = pkg["name"]
    manifest = Path(pkg["manifest_path"]).relative_to(ROOT).as_posix()
    shown = run(["git", "show", f"refs/tags/{tag}:{manifest}"])
    if shown.returncode != 0:
        return []  # a crate the baseline does not have promises nothing yet
    now = set(pkg["features"]) - {"default"}
    sets = [["--only-explicit-features"]]
    sets += [["--only-explicit-features", "--features", f] for f in features(shown.stdout) if f in now]
    sets += [["--all-features"]]
    out = []
    for flags in sets:
        checked = run(["cargo", "semver-checks", "-p", name, "--baseline-rev", tag, *flags])
        if checked.returncode != 0:
            said = [line for line in (checked.stdout + checked.stderr).splitlines()
                    if line.startswith(("--- failure", "error")) or line.strip().startswith(("Summary", "Failed in"))
                    or re.match(r"^\s{2}\S", line)]
            out.append(f"  {name} {' '.join(flags)} breaks {tag}'s API:")
            out += [f"    {line.strip()}" for line in said[:30]]
    return out


def set_aside(tag: str) -> tuple:
    """`SET_ASIDE`'s test names for `tag`, and a line for each entry of another tag or
    naming no test the tag's `crates/mestolo/tests/` has."""
    names, stale = [], []
    for t, name in SET_ASIDE:
        found = run(["git", "grep", "-q", "-E", rf"^fn {name}\(", f"refs/tags/{t}", "--", "crates/mestolo/tests"])
        if t != tag or found.returncode != 0:
            stale.append(f"  SET_ASIDE's ({t}, {name}) is not a test of the baseline {tag}")
        else:
            names.append(name)
    return names, stale


def commands(tag: str) -> list[str]:
    """The baseline's `crates/mestolo/tests/` against the current code, but those set aside."""
    skips, stale = set_aside(tag)
    if run(["git", "cat-file", "-e", f"refs/tags/{tag}:crates/mestolo/tests"]).returncode != 0:
        return stale
    shutil.rmtree(COPY, ignore_errors=True)
    for path in committable():
        rel = path.relative_to(ROOT)
        if rel.parts[:3] == ("crates", "mestolo", "tests"):
            continue
        (COPY / rel).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, COPY / rel)
    archive = subprocess.run(["git", "archive", f"refs/tags/{tag}", "crates/mestolo/tests"],
                             cwd=ROOT, capture_output=True)
    unpacked = subprocess.run(["tar", "-x", "-C", str(COPY)], input=archive.stdout, capture_output=True)
    if archive.returncode != 0 or unpacked.returncode != 0:
        return stale + [f"  {tag}'s crates/mestolo/tests could not be read"]
    skip = [w for name in skips for w in ("--skip", name)]
    tested = run(["cargo", "test", "--quiet", "-p", "mestolo", "--all-features", "--test", "*",
                  "--target-dir", str(ROOT / "target" / "semver" / "target"),
                  "--", "--exact", *skip], cwd=COPY)
    if tested.returncode != 0:
        said = [line for line in (tested.stdout + tested.stderr).splitlines()
                if re.match(r"^(test .* FAILED|error|---- )", line) or "panicked" in line]
        return stale + [f"  {tag}'s mestolo tests fail on the current code (ARCHITECTURE.md §6.3):"] + \
               [f"    {line}" for line in said[:20]]
    return stale


def main() -> int:
    failures = []
    current = version_of((ROOT / "Cargo.toml").read_text())
    tag, base = baseline()
    notes = sections()
    skipped = None
    if not current or current[0] == 0:
        skipped = "before 1.0"
    elif not tag or base[0] == 0:
        skipped = f"no release tag of 1.0 or above merged into HEAD (newest: {tag or 'none'})"
    else:
        stated = ".".join(map(str, current))
        if current != base and stated in notes and BREAKING.search(notes[stated]) and current[0] <= base[0]:
            failures.append(f"  {stated} declares a break (### Breaking) and does not raise {tag}'s MAJOR")
        if current[0] != base[0]:
            skipped = f"{'.'.join(map(str, current))} raises {tag}'s MAJOR"
        elif BREAKING.search(notes.get("Unreleased", "")):
            skipped = "CHANGELOG.md's Unreleased declares a break, for the next MAJOR"
    if failures or skipped:
        if failures:
            print("\n".join(failures))
        if skipped:
            print(f"  SKIPPED: {skipped}")
        return 1 if failures else 0
    for pkg in members():
        failures += library(pkg, tag)
    failures += commands(tag)
    if failures:
        print("\n".join(failures))
        return 1
    print(f"  PASS (against {tag})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
