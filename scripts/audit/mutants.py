#!/usr/bin/env python3
"""Mutation testing at a milestone's close (TESTING.md T11, ADR-0023).

With no argument, the gate's check of `.cargo/mutants.toml`: every pattern in
`exclude_re` is a single-quoted string on a line of its own, under a comment saying why
the mutants it matches are equivalent, and excludes at least one mutant, by cargo-mutants'
own regex engine. A pattern that matches nothing is stale, like an unfulfilled
`#[expect]`.

With `--run [ARGS…]`, the milestone-close run: `cargo mutants --workspace` behind
`rss_guard.py`, with its copies of the tree under `target/`. It fails when a mutant is
missed, or when a mutant timed out before any test failed, since only a failed test shows
that the timeout was the mutant's doing. ARGS go to cargo-mutants (`-f <file>`,
`--iterate`). With `--report DIR`, the same verdict on a `mutants.out` already written.
"""

import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
CONFIG = ROOT / ".cargo" / "mutants.toml"
OUT = ROOT / "out" / "mutants"
GUARD = ROOT / "scripts" / "audit" / "rss_guard.py"
# Thirty times the unmutated suite's largest peak, 32 MiB (2026-10-18, ADR-0023).
GUARD_MIB = 1024
# Two jobs: four, with no guard, took the machine down (ADR-0023).
JOBS = 2


def patterns() -> tuple[list[str], list[str]]:
    """The `exclude_re` patterns, and what is wrong with how they are written."""
    lines = CONFIG.read_text().splitlines()
    starts = [i for i, l in enumerate(lines) if l.strip().startswith("exclude_re")]
    if len(starts) != 1 or not lines[starts[0]].rstrip().endswith("["):
        return [], ["  .cargo/mutants.toml: no 'exclude_re = [' line"]
    found, errors = [], []
    previous = ""
    for n, raw in enumerate(lines[starts[0] + 1:], start=starts[0] + 2):
        line = raw.strip()
        if line == "]":
            return found, errors
        if line.startswith("#"):
            previous = line
            continue
        if not (line.startswith("'") and line.endswith("',") and line.count("'") == 2):
            errors.append(f"  .cargo/mutants.toml:{n}: not a single-quoted pattern on its own line")
        elif not previous.startswith("#"):
            errors.append(f"  .cargo/mutants.toml:{n}: no comment above saying why it is equivalent")
        else:
            found.append(line[1:-2])
        previous = line
    return found, errors + ["  .cargo/mutants.toml: 'exclude_re' is not closed"]


def count(*args: str) -> int:
    out = subprocess.run(
        ["cargo", "mutants", "--workspace", "--list", "--no-config", *args],
        cwd=ROOT, capture_output=True, text=True, check=True,
    ).stdout
    return len(out.splitlines())


def check() -> int:
    found, errors = patterns()
    every = count()
    for p in found:
        if count("--exclude-re", p) == every:
            errors.append(f"  .cargo/mutants.toml: matches no mutant: '{p}'")
    if errors:
        print("\n".join(errors))
        return 1
    print(f"  PASS ({len(found)} patterns, each with its reason and a mutant; {every} mutants)")
    return 0


def host() -> str:
    out = subprocess.run(["rustc", "-vV"], capture_output=True, text=True, check=True).stdout
    return next(l.split()[1] for l in out.splitlines() if l.startswith("host:"))


def run(args: list[str]) -> int:
    tmp = ROOT / "target" / "mutants-tmp"
    shutil.rmtree(tmp, ignore_errors=True)
    tmp.mkdir(parents=True)
    OUT.mkdir(parents=True, exist_ok=True)
    log = OUT / "rss_guard.log"
    log.unlink(missing_ok=True)
    env = dict(os.environ)
    env[f"CARGO_TARGET_{host().upper().replace('-', '_')}_RUNNER"] = str(GUARD)
    env["RSS_GUARD_MIB"] = str(GUARD_MIB)
    env["RSS_GUARD_LOG"] = str(log)
    env["TMPDIR"] = str(tmp)
    subprocess.run(["cargo", "mutants", "--workspace", "-j", str(JOBS), *args], cwd=ROOT, env=env,
                   check=False)
    failures = verdict(OUT / "mutants.out")
    if not (log.exists() and log.read_text().strip()):
        failures.append("  rss_guard.py logged nothing: the test binaries ran unguarded")
    return report(failures)


def verdict(out: Path) -> list[str]:
    """What a cargo-mutants report shows was not caught."""
    failures = []
    outcomes = json.loads((out / "outcomes.json").read_text())["outcomes"]
    for o in outcomes:
        if o["scenario"] == "Baseline":
            continue
        name = o["scenario"]["Mutant"]["name"]
        if o["summary"] == "MissedMutant":
            failures.append(f"  missed: {name}")
        elif o["summary"] == "Timeout":
            text = (out / o["log_path"]).read_text(errors="replace")
            if not any(l.startswith("test ") and l.endswith(" ... FAILED") for l in text.splitlines()):
                failures.append(f"  timed out before any test failed: {name}")
    print(f"  {len(outcomes) - 1} mutants reported")
    return failures


def report(failures: list[str]) -> int:
    if failures:
        print("\n".join(failures))
        print("  kill each with a test (T3), or list it in .cargo/mutants.toml with its reason")
        return 1
    print("  PASS (none missed)")
    return 0


def main() -> int:
    if shutil.which("cargo-mutants") is None:
        print("  cargo-mutants is not installed: cargo install --locked cargo-mutants@27.1.0")
        return 1
    if sys.argv[1:2] == ["--run"]:
        return run(sys.argv[2:])
    if sys.argv[1:2] == ["--report"] and len(sys.argv) == 3:
        return report(verdict(Path(sys.argv[2]).resolve()))
    return check()


if __name__ == "__main__":
    sys.exit(main())
