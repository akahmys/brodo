#!/usr/bin/env python3
"""Run a test binary and kill it once its resident memory passes a limit.

cargo runs it as the target's runner (CARGO_TARGET_<TRIPLE>_RUNNER), which `mutants.py`
sets, so it sits in front of every test binary cargo-mutants starts (TESTING.md T11). It
counts and kills the binary together with every process descended from it.
macOS does not enforce `ulimit -v`, `-d` or `-m`, and a mutant that makes an iterator
endless grows without bound until the machine panics (ADR-0023).

Environment:
  RSS_GUARD_MIB  the limit in MiB; required, so that no run goes unguarded by mistake
  RSS_GUARD_LOG  a file to which one line per run is appended: peak MiB, verdict, binary
"""

import os
import signal
import subprocess
import sys
import time

POLL_SECONDS = 0.1


def tree(root: int) -> dict[int, int]:
    """Resident KiB of `root` and every process descended from it, by pid. Followed by
    parent pid, not by process group: the binary stays in the group cargo-mutants made,
    which it signals on a timeout (`killpg`)."""
    out = subprocess.run(
        ["ps", "-A", "-o", "pid=,ppid=,rss="], capture_output=True, text=True, check=False
    ).stdout
    children: dict[int, list[int]] = {}
    rss: dict[int, int] = {}
    for line in out.splitlines():
        fields = line.split()
        if len(fields) == 3:
            pid, ppid, kib = map(int, fields)
            children.setdefault(ppid, []).append(pid)
            rss[pid] = kib
    found: dict[int, int] = {}
    stack = [root]
    while stack:
        pid = stack.pop()
        if pid in rss and pid not in found:
            found[pid] = rss[pid]
            stack.extend(children.get(pid, []))
    return found


def kill(pids) -> None:
    for pid in pids:
        try:
            os.kill(pid, signal.SIGKILL)
        except ProcessLookupError:
            pass


def main() -> int:
    limit_kib = int(os.environ["RSS_GUARD_MIB"]) * 1024
    log = os.environ.get("RSS_GUARD_LOG")
    child = subprocess.Popen(sys.argv[1:])
    peak = 0
    verdict = "ok"
    seen: dict[int, int] = {}
    while child.poll() is None:
        seen = tree(child.pid)
        peak = max(peak, sum(seen.values()))
        if peak > limit_kib:
            kill(seen)
            child.wait()
            verdict = "killed"
            print(
                f"rss_guard: killed {sys.argv[1]} and {len(seen) - 1} descendants at "
                f"{peak // 1024} MiB (limit {limit_kib // 1024} MiB)",
                file=sys.stderr,
            )
            break
        time.sleep(POLL_SECONDS)
    # Descendants the binary left behind when it ended.
    kill(pid for pid in seen if pid != child.pid)
    if log:
        with open(log, "a", encoding="utf-8") as f:
            f.write(f"{peak // 1024}\t{verdict}\t{os.path.basename(sys.argv[1])}\n")
    return 137 if verdict == "killed" else child.returncode


if __name__ == "__main__":
    sys.exit(main())
