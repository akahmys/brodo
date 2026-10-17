#!/usr/bin/env python3
"""Run a test binary and kill it once its resident memory passes a limit.

cargo runs it as the target's runner (CARGO_TARGET_<TRIPLE>_RUNNER), which `mutants.py`
sets, so it sits in front of every test binary cargo-mutants starts (TESTING.md T11).
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


def rss_kib(pid: int) -> int:
    out = subprocess.run(
        ["ps", "-o", "rss=", "-p", str(pid)], capture_output=True, text=True, check=False
    ).stdout.strip()
    return int(out) if out else 0


def main() -> int:
    limit_kib = int(os.environ["RSS_GUARD_MIB"]) * 1024
    log = os.environ.get("RSS_GUARD_LOG")
    child = subprocess.Popen(sys.argv[1:])
    peak = 0
    verdict = "ok"
    while child.poll() is None:
        peak = max(peak, rss_kib(child.pid))
        if peak > limit_kib:
            child.send_signal(signal.SIGKILL)
            child.wait()
            verdict = "killed"
            print(
                f"rss_guard: killed {sys.argv[1]} at {peak // 1024} MiB "
                f"(limit {limit_kib // 1024} MiB)",
                file=sys.stderr,
            )
            break
        time.sleep(POLL_SECONDS)
    if log:
        with open(log, "a", encoding="utf-8") as f:
            f.write(f"{peak // 1024}\t{verdict}\t{os.path.basename(sys.argv[1])}\n")
    return 137 if verdict == "killed" else child.returncode


if __name__ == "__main__":
    sys.exit(main())
