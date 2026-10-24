#!/usr/bin/env python3
"""Mutation testing at a milestone's close (TESTING.md T11, ADR-0023).

With no argument, the gate's check of `.cargo/mutants.toml`, which holds the exceptions
and nothing else: every pattern in
`exclude_re` is a single-quoted string on a line of its own, under a comment saying why
the mutants it matches are equivalent, and excludes at least one mutant, by cargo-mutants'
own regex engine. A pattern that matches nothing is stale, like an unfulfilled
`#[expect]`.

With `--run [ARGS…]`, the milestone-close run: `cargo mutants --workspace` behind
`rss_guard.py`, with its copies of the tree under `target/`. It fails when a mutant is
missed, or when a mutant timed out before any test failed, since only a failed test shows
that the timeout was the mutant's doing. ARGS go to cargo-mutants (`-f <file>`,
`--iterate`, `--shard k/n`). With `--report DIR [DIR…]`, the same verdict on reports
already written, one or a shard each, taken together.

With `--run --patient`, the mutants the last report shows timing out before any test
failed run again, alone, under `PATIENT_TIMEOUT` seconds each, into `out/mutants/patient`:
one caught is killed; one whose tests all pass is only slower, and is listed in
`.cargo/mutants.toml` as equivalent with that reason; one still timing out fails. The
verdict is the last report's missed mutants and the patient run's (ADR-0023). With
`--patient --from DIR [DIR…] [-- ARGS]`, the reports are those, a sharded run's.
"""

import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
CONFIG = ROOT / ".cargo" / "mutants.toml"
OUT = ROOT / "out" / "mutants"
GUARD = ROOT / "scripts" / "audit" / "rss_guard.py"
# A key at the start of a line: the file holds the exceptions only, so that a setting of
# the run, which is a rule, is changed here with an ADR (AGENTS.md, "Changing the rules").
SETTING = re.compile(r"^\s*[\w.\-\[]+\s*(=|\])")
# Thirty times the unmutated suite's largest peak, 32 MiB (2026-10-18, ADR-0023).
GUARD_MIB = 1024
# Two jobs: four, with no guard, took the machine down (ADR-0023).
JOBS = 2
# Twice the unmutated suite's time: each mutant still running then had already failed a
# test (ADR-0023).
TIMEOUT_MULTIPLIER = "2.0"
# The patient run's timeout, in seconds: long enough for a search made exhaustive, such as
# a bounding-volume test that never prunes, to finish the suite (ADR-0023).
PATIENT_TIMEOUT = "1800"
PATIENT_OUT = ROOT / "out" / "mutants" / "patient"


def patterns() -> tuple[list[str], list[str]]:
    """The `exclude_re` patterns, and what is wrong with how they are written."""
    lines = CONFIG.read_text().splitlines()
    settings = [n for n, l in enumerate(lines, start=1)
                if SETTING.match(l) and not l.strip().startswith("exclude_re")]
    if settings:
        return [], [f"  .cargo/mutants.toml:{n}: a setting; only 'exclude_re' belongs here, "
                    "the run's settings are in mutants.py" for n in settings]
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


def mutants(args: list[str], out: Path) -> None:
    """Runs cargo-mutants over the workspace behind the guard, writing to `out`."""
    tmp = ROOT / "target" / "mutants-tmp"
    shutil.rmtree(tmp, ignore_errors=True)
    tmp.mkdir(parents=True)
    out.mkdir(parents=True, exist_ok=True)
    log = out / "rss_guard.log"
    log.unlink(missing_ok=True)
    env = dict(os.environ)
    env[f"CARGO_TARGET_{host().upper().replace('-', '_')}_RUNNER"] = str(GUARD)
    env["RSS_GUARD_MIB"] = str(GUARD_MIB)
    env["RSS_GUARD_LOG"] = str(log)
    env["TMPDIR"] = str(tmp)
    subprocess.run(["cargo", "mutants", "--workspace", "-j", str(JOBS), "-o", str(out), *args],
                   cwd=ROOT, env=env, check=False)


def guarded(out: Path) -> list[str]:
    log = out / "rss_guard.log"
    if not (log.exists() and log.read_text().strip()):
        return ["  rss_guard.py logged nothing: the test binaries ran unguarded"]
    return []


def run(args: list[str]) -> int:
    if args[:1] == ["--patient"]:
        return patient(args[1:])
    mutants(["--timeout-multiplier", TIMEOUT_MULTIPLIER, *args], OUT)
    return report(verdict(OUT / "mutants.out") + guarded(OUT))


def escaped(name: str) -> str:
    """A mutant's name as a regex matching it alone; a space needs no escape."""
    return re.sub(r"([\\.+*?()|\[\]{}^$#&\-~])", r"\\\1", name)


def patient(args: list[str]) -> int:
    """The last report's unexplained timeouts, run again under `PATIENT_TIMEOUT`."""
    reports = [OUT / "mutants.out"]
    if args[:1] == ["--from"]:
        rest = args[1:]
        split = rest.index("--") if "--" in rest else len(rest)
        reports, args = [Path(d).resolve() for d in rest[:split]], rest[split + 1:]
    failures_before = verdicts(reports)
    missed = [f for f in failures_before if f.startswith("  missed")]
    prefix = "  timed out before any test failed: "
    slow = [f.removeprefix(prefix) for f in failures_before if f.startswith(prefix)]
    if not slow:
        return report(missed)
    pattern = "^(" + "|".join(escaped(n) for n in slow) + ")$"
    mutants(["--timeout", PATIENT_TIMEOUT, "--re", pattern, *args], PATIENT_OUT)
    failures = []
    outcomes = json.loads((PATIENT_OUT / "mutants.out" / "outcomes.json").read_text())
    for o in outcomes["outcomes"]:
        if o["scenario"] == "Baseline":
            continue
        name = o["scenario"]["Mutant"]["name"]
        # `--re` lets a deleted struct field through, whatever its name: only the named count.
        if name not in slow:
            continue
        if o["summary"] == "MissedMutant":
            failures.append(f"  only slower, its tests passing: {name}")
        elif o["summary"] == "Timeout":
            failures.append(f"  still timing out after {PATIENT_TIMEOUT} s: {name}")
    print(f"  {len(slow)} timeouts run again under {PATIENT_TIMEOUT} s")
    return report(missed + failures + guarded(PATIENT_OUT))


def verdicts(outs: list[Path]) -> list[str]:
    """What several reports, the shards of one run, show was not caught, each once."""
    seen: list[str] = []
    for out in outs:
        seen += [f for f in verdict(out) if f not in seen]
    return seen


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
    if sys.argv[1:2] == ["--report"] and len(sys.argv) >= 3:
        return report(verdicts([Path(d).resolve() for d in sys.argv[2:]]))
    return check()


if __name__ == "__main__":
    sys.exit(main())
