#!/usr/bin/env python3
"""Checks on code that clippy has no lint for (CODING.md Rules 2, 4, 8, 9, 15, 18 and 19,
TESTING.md T1).

Production code is each `src/` file up to its test module, an unindented `#[cfg(test)]`
followed by `mod tests`, which clippy's `items_after_test_module` keeps at the end of the
file.
An item gated by `#[cfg(test)]` before it, such as a helper the tests call, is read as
production code.

1. Rule 2: no `assert!`, `assert_eq!` or `assert_ne!`. `debug_assert!` is allowed.
2. Rule 15: no recursion. A cycle in a crate's call graph fails unless one function in it
   carries `// Bounded recursion: <the bound>` among the comments and attributes directly
   above its `fn`.

3. Rule 19: an `#[expect]`, written directly or under `cfg_attr`, in any target (tests,
   examples, benchmarks and a `src/` file's test module included), names only a lint
   CODING.md lets an exception cover, the print lints only in an application crate, and `unsafe_code` only in the file
   `UNSAFE_MODULES` lists or the files `UNSAFE_CALLERS` lists (`project.py`). clippy holds the rest of Rule 19 (`expect` rather
   than `allow`, and a reason).
4. Rule 4: the word `unsafe` appears in code (comments and strings aside) only in that
   file, in every `.rs` file of every member, tests included, and `UNSAFE_MODULES` has at
   most one entry; in a file `UNSAFE_CALLERS` lists, it appears only as
   `unsafe { CacheFile::map(`, a call of the one `unsafe fn` brodo offers (ADR-0036).
   rustc's `unsafe_code = "deny"` holds the rest: unsafe code without the `#[expect]` fails.

5. Rule 8, with `--errors`: in a library crate's `src/`, no `Result` whose error is
   `String`, however its types nest or its lines break, and no `anyhow`.
6. Rule 9, with `--errors`: no `filter_map` or `flat_map` of `Result::ok`, by any path, or
   of a closure giving its argument's `.ok()`, however its lines break.
7. T1, with `--ignores`: in every target, an attribute naming `ignore`, directly or under
   `cfg_attr`, however its lines break, is `ignore = "needs …"`.

The call graph is built from names, so it is approximate. A call counts when it is a bare
`name(`, `self.name(` or `Self::name(`; `Vec::new()` inside `fn new` is not a call to
it, and neither is `other.name(` nor `Type::name(`, since without types a method's name
would join every function of that name. Two functions sharing a name in one crate are one
node, which can report a cycle that is not there; rename one. Recursion through a closure,
a trait object, a method called on another receiver or a path is not seen.
"""

import re
import sys
from pathlib import Path

from project import APPLICATIONS, UNSAFE_CALLERS, UNSAFE_MODULES

ROOT = Path(__file__).resolve().parent.parent.parent

ASSERT = re.compile(r"(?<![\w:])(?:std::|core::)?assert(?:_eq|_ne)?!")
FN = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const\s+)?(?:async\s+)?fn\s+(\w+)")
MARKER = re.compile(r"^\s*// Bounded recursion: \S")
# Directly, or as the attribute a `cfg_attr` applies, its condition never holding a `]`.
LINT_ATTRIBUTE = re.compile(
    r"#!?\[\s*(?:cfg_attr\s*\([^\]]*?,\s*)?(?:allow|expect)\s*\((.*?)\)\s*\)?\s*\]", re.DOTALL
)
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
# They may print (Rule 18) and return untyped errors (Rule 8).
APPLICATION_CRATES = set(APPLICATIONS) | {"xtask"}
UNSAFE_LINT = "unsafe_code"  # Rule 4, in UNSAFE_MODULES only
UNSAFE = re.compile(r"\bunsafe\b")
CALL = re.compile(r"(?:(?<![\w.:])|(?<=self\.)|(?<=Self::))(\w+)\s*\(")


def production(path: Path) -> list[str]:
    lines = path.read_text().split("\n")
    for i, line in enumerate(lines):
        following = lines[i + 1 : i + 2]
        if line == "#[cfg(test)]" and following and re.match(r"mod tests\b", following[0]):
            return lines[:i]
    return lines


def crates() -> list[Path]:
    roots = [p.parent for p in (ROOT / "crates").glob("*/src")]
    if (ROOT / "xtask" / "src").is_dir():
        roots.append(ROOT / "xtask")
    return sorted(roots)


# A string, a character, or a line's comment: what `blank` empties.
LEXEME = re.compile(r'"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])\'|//.*')


def blank(line: str) -> str:
    """A line with each string's and character's text made spaces, their quotes kept, and
    its comment made spaces: its code alone, every position where it was. A string is found
    before a comment, so `"http://…"` is a string."""
    def empty(m: re.Match[str]) -> str:
        text = m.group(0)
        return " " * len(text) if text.startswith("//") else text[0] + " " * (len(text) - 2) + text[-1]

    return LEXEME.sub(empty, line)


def asserts(files: list[Path]) -> list[str]:
    out = []
    for path in files:
        for number, line in enumerate(production(path), 1):
            code = blank(line)
            if ASSERT.search(code):
                out.append(f"  {path.relative_to(ROOT)}:{number}: assert in production code (Rule 2)")
    return out


def exceptions(crate: Path, files: list[Path]) -> list[str]:
    out = []
    allowed = EXEMPTABLE | (PRINTING if crate.name in APPLICATION_CRATES else set())
    for path in files:
        text = path.read_text()
        may = unsafe_allowed(path) or caller(path)
        here = allowed | ({UNSAFE_LINT} if may else set())
        for match in LINT_ATTRIBUTE.finditer(text):
            number = text.count("\n", 0, match.start()) + 1
            for part in match.group(1).split(","):
                if "=" in part or not LINT_NAME.match(part):
                    continue
                lint = part.strip()
                if lint not in here:
                    out.append(f"  {path.relative_to(ROOT)}:{number}: {lint} may not be excepted (Rule 19)")
    return out


def unsafe_allowed(path: Path) -> bool:
    return str(path.relative_to(ROOT)) in UNSAFE_MODULES


def caller(path: Path) -> bool:
    return str(path.relative_to(ROOT)) in UNSAFE_CALLERS


MAP_CALL = re.compile(r"\bunsafe \{ CacheFile::map\(")


def unsafe_code(crate: Path) -> list[str]:
    out = []
    for path in sorted(crate.rglob("*.rs")):
        if "target" in path.relative_to(crate).parts or unsafe_allowed(path):
            continue
        for number, line in enumerate(path.read_text().split("\n"), 1):
            code = blank(line)
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
                calls[current] |= set(CALL.findall(blank(line)))
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


def code(path: Path) -> str:
    """A file's text with each line's comment and strings blanked, positions kept."""
    return "\n".join(blank(line) for line in path.read_text().split("\n"))


def line_of(text: str, at: int) -> int:
    return text.count("\n", 0, at) + 1


def closing(text: str, at: int, open_: str, close: str) -> int:
    """The index just past the bracket closing the one at `at`, or the text's end."""
    depth = 0
    for i in range(at, len(text)):
        if text[i] == open_:
            depth += 1
        elif text[i] == close and not arrow(text, i):
            depth -= 1
            if depth == 0:
                return i + 1
    return len(text)


def arrow(text: str, i: int) -> bool:
    """Whether the `>` at `i` ends `->` or `=>`, not a bracket."""
    return text[i] == ">" and i > 0 and text[i - 1] in "-="


def arguments(inside: str) -> list[str]:
    """Generic arguments split at their top-level commas."""
    out, depth, start = [], 0, 0
    for i, c in enumerate(inside):
        if c in "<([":
            depth += 1
        elif c in ">)]" and not arrow(inside, i):
            depth -= 1
        elif c == "," and depth == 0:
            out.append(inside[start:i])
            start = i + 1
    # A trailing comma, as `cargo fmt` writes one when the arguments break over lines.
    return out + [inside[start:]] if inside[start:].strip() else out


RESULT = re.compile(r"\bResult\s*<")
STRING_TYPE = re.compile(r"^(?:(?:std|alloc)::string::)?String$")
ANYHOW = re.compile(r"\banyhow\b")
OK_PATH = re.compile(
    r"\b(?:filter_map|flat_map)\s*\(\s*(?:(?:std|core)\s*::\s*result\s*::\s*)?Result\s*"
    r"(?:::\s*<[^>]*>\s*)?::\s*ok\s*,?\s*\)"
)
OK_CLOSURE = re.compile(
    r"\b(?:filter_map|flat_map)\s*\(\s*(?:move\s*)?\|\s*(\w+)\s*(?::[^|]*)?\|\s*\1\s*\.\s*ok\s*\(\s*\)\s*,?\s*\)"
)


def errors(crate: Path, files: list[Path]) -> list[str]:
    """Rule 8 in a library crate, and Rule 9 in every crate."""
    out = []
    for path in files:
        text, where = code(path), path.relative_to(ROOT)
        if crate.name not in APPLICATION_CRATES:
            for m in RESULT.finditer(text):
                end = closing(text, m.end() - 1, "<", ">")
                parts = arguments(text[m.end() : end - 1])
                if len(parts) == 2 and STRING_TYPE.match(re.sub(r"\s+", "", parts[1])):
                    out.append(f"  {where}:{line_of(text, m.start())}: String as an error (Rule 8)")
            out += [f"  {where}:{line_of(text, m.start())}: anyhow (Rule 8)" for m in ANYHOW.finditer(text)]
        for m in [*OK_PATH.finditer(text), *OK_CLOSURE.finditer(text)]:
            out.append(f"  {where}:{line_of(text, m.start())}: an error filtered away (Rule 9)")
    return out


ATTRIBUTE = re.compile(r"#!?\[")


def ignores(files: list[Path]) -> list[str]:
    """T1: each attribute naming `ignore` is `ignore = "needs …"`, under `cfg_attr` or not."""
    out = []
    for path in files:
        raw = path.read_text()
        # Found in the code alone; a reason read from the text at the same place.
        text = code(path)
        for m in ATTRIBUTE.finditer(text):
            end = closing(text, m.end() - 1, "[", "]")
            words = text[m.end() - 1 : end]
            if not re.search(r"\bignore\b", words) or not re.match(r"\[\s*(?:ignore|cfg_attr)\b", words):
                continue
            reasons = [r for r in re.finditer(r'\bignore\s*=\s*"', words)
                       if raw.startswith('"needs ', m.end() - 1 + r.end() - 1)]
            if not reasons:
                out.append(f"  {path.relative_to(ROOT)}:{line_of(raw, m.start())}: "
                           "a test is switched off without naming what it needs (T1)")
    return out


def every_target(crate: Path) -> list[Path]:
    return [p for p in sorted(crate.rglob("*.rs")) if "target" not in p.relative_to(crate).parts]


def main() -> int:
    if sys.argv[1:] in (["--errors"], ["--ignores"]):
        found = []
        for crate in crates():
            if sys.argv[1] == "--errors":
                found += errors(crate, sorted(crate.rglob("src/**/*.rs")))
            else:
                found += ignores(every_target(crate))
        print("\n".join(found) if found else "  PASS")
        return 1 if found else 0
    failures = []
    if len(UNSAFE_MODULES) > 1:
        failures.append(f"  UNSAFE_MODULES lists {len(UNSAFE_MODULES)} files; Rule 4 allows one")
    failures += [f"  {m} (UNSAFE_MODULES) does not exist" for m in UNSAFE_MODULES
                 if not (ROOT / m).is_file()]
    for crate in crates():
        files = sorted(crate.rglob("src/**/*.rs"))
        failures += asserts(files)
        # Rule 19 holds in every target: tests, examples and benchmarks too.
        failures += exceptions(crate, every_target(crate))
        failures += recursion(crate)
        failures += unsafe_code(crate)
    if failures:
        print("\n".join(failures))
        return 1
    print("  PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
