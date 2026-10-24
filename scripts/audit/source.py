#!/usr/bin/env python3
"""Checks on code that clippy has no lint for (CODING.md Rules 2, 4, 15 and 19).

Production code is each `src/` file up to its `#[cfg(test)]` module, which clippy's
`items_after_test_module` keeps at the end of the file.

1. Rule 2: no `assert!`, `assert_eq!` or `assert_ne!`. `debug_assert!` is allowed.
2. Rule 15: no recursion. A cycle in a crate's call graph fails unless one function in it
   carries `// Bounded recursion: <the bound>` among the comments and attributes directly
   above its `fn`.

3. Rule 19: an `#[expect]`, in any target (tests, examples and benchmarks included), names
   only a lint CODING.md lets an exception cover, the print lints only in an application crate, and `unsafe_code` only in the file
   `UNSAFE_MODULES` lists or the files `UNSAFE_CALLERS` lists (`project.py`). clippy holds the rest of Rule 19 (`expect` rather
   than `allow`, and a reason).
4. Rule 4: the word `unsafe` appears in code (comments and strings aside) only in that
   file, in every `.rs` file of every member, tests included, and `UNSAFE_MODULES` has at
   most one entry; in a file `UNSAFE_CALLERS` lists, it appears only as
   `unsafe { CacheFile::map(`, a call of the one `unsafe fn` brodo offers (ADR-0036).
   rustc's `unsafe_code = "deny"` holds the rest: unsafe code without the `#[expect]` fails.

The call graph is built from names, so it is approximate. A call counts when it is a bare
`name(`, `self.name(` or `Self::name(`; `Vec::new()` inside `fn new` is not a call to
it. Two functions sharing a name in one crate are one node, which can report a cycle that
is not there; rename one. Recursion through a closure or a trait object is not seen.
"""

import re
import sys
from pathlib import Path

from project import APPLICATIONS, UNSAFE_CALLERS, UNSAFE_MODULES

ROOT = Path(__file__).resolve().parent.parent.parent

ASSERT = re.compile(r"(?<![\w:])(?:std::|core::)?assert(?:_eq|_ne)?!")
FN = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const\s+)?(?:async\s+)?fn\s+(\w+)")
MARKER = re.compile(r"^\s*// Bounded recursion: \S")
LINT_ATTRIBUTE = re.compile(r"#!?\[\s*(?:allow|expect)\s*\((.*?)\)\s*\]", re.DOTALL)
LINT_NAME = re.compile(r"^\s*([\w:]+)\s*$")
# The lints an exception may cover, by the rule that allows it.
EXEMPTABLE = {
    "clippy::too_many_lines",  # Rule 1
    "clippy::unwrap_used", "clippy::expect_used", "clippy::panic",  # Rule 2
    "clippy::unreachable", "clippy::todo", "clippy::unimplemented",  # Rule 2
    "clippy::wildcard_enum_match_arm", "clippy::match_wildcard_for_single_variants",  # Rule 7
    "clippy::too_many_arguments",  # Rule 10
    "clippy::cast_possible_truncation", "clippy::cast_possible_wrap",  # Rule 17
    "clippy::cast_sign_loss", "clippy::float_cmp",  # Rule 17
}
PRINTING = {"clippy::print_stdout", "clippy::print_stderr"}  # Rule 18
MAY_PRINT = set(APPLICATIONS) | {"xtask"}
UNSAFE_LINT = "unsafe_code"  # Rule 4, in UNSAFE_MODULES only
UNSAFE = re.compile(r"\bunsafe\b")
CALL = re.compile(r"(?:(?<![\w.:])|(?<=self\.)|(?<=Self::))(\w+)\s*\(")


def production(path: Path) -> list[str]:
    lines = path.read_text().split("\n")
    for i, line in enumerate(lines):
        if line.strip() == "#[cfg(test)]":
            return lines[:i]
    return lines


def crates() -> list[Path]:
    roots = [p.parent for p in (ROOT / "crates").glob("*/src")]
    if (ROOT / "xtask" / "src").is_dir():
        roots.append(ROOT / "xtask")
    return sorted(roots)


def asserts(files: list[Path]) -> list[str]:
    out = []
    for path in files:
        for number, line in enumerate(production(path), 1):
            code = line.split("//")[0]
            if ASSERT.search(code):
                out.append(f"  {path.relative_to(ROOT)}:{number}: assert in production code (Rule 2)")
    return out


def exceptions(crate: Path, files: list[Path]) -> list[str]:
    out = []
    allowed = EXEMPTABLE | (PRINTING if crate.name in MAY_PRINT else set())
    for path in files:
        in_src = "src" in path.relative_to(crate).parts
        text = "\n".join(production(path) if in_src else path.read_text().split("\n"))
        may = unsafe_allowed(path) or caller(path)
        here = allowed | ({UNSAFE_LINT} if may else set())
        for match in LINT_ATTRIBUTE.finditer(text):
            number = text.count("\n", 0, match.start()) + 1
            for part in match.group(1).split(","):
                if "reason" in part or not LINT_NAME.match(part):
                    continue
                lint = part.strip()
                if lint not in here:
                    out.append(f"  {path.relative_to(ROOT)}:{number}: {lint} may not be excepted (Rule 19)")
    return out


def unsafe_allowed(path: Path) -> bool:
    return str(path.relative_to(ROOT)) in UNSAFE_MODULES


def caller(path: Path) -> bool:
    return str(path.relative_to(ROOT)) in UNSAFE_CALLERS


STRING = re.compile(r'"(?:\\.|[^"\\])*"')
MAP_CALL = re.compile(r"\bunsafe \{ CacheFile::map\(")


def unsafe_code(crate: Path) -> list[str]:
    out = []
    for path in sorted(crate.rglob("*.rs")):
        if "target" in path.relative_to(crate).parts or unsafe_allowed(path):
            continue
        for number, line in enumerate(path.read_text().split("\n"), 1):
            code = STRING.sub('""', line.split("//")[0])
            if caller(path):
                code = MAP_CALL.sub("", code)
            if UNSAFE.search(code):
                out.append(f"  {path.relative_to(ROOT)}:{number}: unsafe outside UNSAFE_MODULES (Rule 4)")
    return out


def call_graph(files: list[Path]) -> tuple[dict, set, dict]:
    calls, bounded, where = {}, set(), {}
    for path in files:
        current, marked = None, False
        for number, line in enumerate(production(path), 1):
            match = FN.match(line)
            stripped = line.strip()
            if match:
                current = match.group(1)
                calls.setdefault(current, set())
                where.setdefault(current, f"{path.relative_to(ROOT)}:{number}")
                if marked:
                    bounded.add(current)
                marked = False
                continue
            if stripped.startswith(("//", "#[")):
                marked = marked or bool(MARKER.match(line))
            elif stripped:
                marked = False
            if current:
                calls[current] |= set(CALL.findall(line.split("//")[0]))
    for name in calls:
        calls[name] &= calls.keys()
    return calls, bounded, where


def cycles(calls: dict) -> list[list[str]]:
    """Strongly connected components with a cycle, by Tarjan's algorithm, iteratively."""
    index, low, on_stack, stack, found, counter = {}, {}, set(), [], [], [0]
    for root in calls:
        if root in index:
            continue
        work = [(root, iter(sorted(calls[root])))]
        index[root] = low[root] = counter[0]
        counter[0] += 1
        stack.append(root)
        on_stack.add(root)
        while work:
            node, children = work[-1]
            child = next(children, None)
            if child is None:
                work.pop()
                if work:
                    low[work[-1][0]] = min(low[work[-1][0]], low[node])
                if low[node] == index[node]:
                    component = []
                    while True:
                        member = stack.pop()
                        on_stack.discard(member)
                        component.append(member)
                        if member == node:
                            break
                    if len(component) > 1 or node in calls[node]:
                        found.append(sorted(component))
            elif child not in index:
                index[child] = low[child] = counter[0]
                counter[0] += 1
                stack.append(child)
                on_stack.add(child)
                work.append((child, iter(sorted(calls[child]))))
            elif child in on_stack:
                low[node] = min(low[node], index[child])
    return found


def recursion(crate: Path) -> list[str]:
    calls, bounded, where = call_graph(sorted(crate.rglob("src/**/*.rs")))
    out = []
    for component in cycles(calls):
        if not bounded & set(component):
            names = ", ".join(f"{n} ({where[n]})" for n in component)
            out.append(f"  recursion without a stated bound (Rule 15): {names}")
    return out


def main() -> int:
    failures = []
    if len(UNSAFE_MODULES) > 1:
        failures.append(f"  UNSAFE_MODULES lists {len(UNSAFE_MODULES)} files; Rule 4 allows one")
    failures += [f"  {m} (UNSAFE_MODULES) does not exist" for m in UNSAFE_MODULES
                 if not (ROOT / m).is_file()]
    for crate in crates():
        files = sorted(crate.rglob("src/**/*.rs"))
        failures += asserts(files)
        # Rule 19 holds in every target: tests, examples and benchmarks too.
        every = [p for p in sorted(crate.rglob("*.rs")) if "target" not in p.relative_to(crate).parts]
        failures += exceptions(crate, every)
        failures += recursion(crate)
        failures += unsafe_code(crate)
    if failures:
        print("\n".join(failures))
        return 1
    print("  PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
