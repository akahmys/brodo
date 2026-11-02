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

from paths import committable
from project import APPLICATIONS, UNSAFE_CALLERS, UNSAFE_MODULES

ROOT = Path(__file__).resolve().parent.parent.parent

ASSERT = re.compile(r"(?<![\w:])(?:std::|core::)?assert(?:_eq|_ne)?!")
FN = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const\s+)?(?:async\s+)?fn\s+(\w+)")
MARKER = re.compile(r"^\s*// Bounded recursion: \S")
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


def blank(text: str) -> str:
    """Text with each string's and character's contents and each comment made spaces, its
    quotes and line ends kept: its code alone, every character where it was. Read by a
    lexer, left to right, as rustc reads it: a line comment; a block comment, nested; a raw
    string, `r#"…"#`, `br…` or `cr…`; a string or byte string, over lines too; a character
    or byte, `'x'`, `'\\n'`, `'\\u{…}'`, told from a lifetime, `'a`, which is code."""
    out = list(text)
    n, i = len(text), 0

    def empty(start: int, end: int) -> None:
        for k in range(start, min(end, n)):
            if out[k] != "\n":
                out[k] = " "

    while i < n:
        c = text[i]
        two = text[i : i + 2]
        if two == "//":
            end = text.find("\n", i)
            end = n if end < 0 else end
            empty(i, end)
            i = end
        elif two == "/*":
            depth, k = 1, i + 2
            while k < n and depth:
                if text.startswith("/*", k):
                    depth, k = depth + 1, k + 2
                elif text.startswith("*/", k):
                    depth, k = depth - 1, k + 2
                else:
                    k += 1
            empty(i, k)
            i = k
        elif (raw := re.match(r'(?:b|c)?r(#*)"', text[i:])) and not ident_before(text, i):
            close = '"' + raw.group(1)
            start = i + raw.end()
            end = text.find(close, start)
            end = n if end < 0 else end
            empty(start, end)
            i = end + len(close)
        elif c == '"' or (two in ('b"', 'c"') and not ident_before(text, i)):
            k = i + (2 if c != '"' else 1)
            start = k
            while k < n and text[k] != '"':
                k += 2 if text[k] == "\\" else 1
            empty(start, k)
            i = k + 1
        elif c == "'" or (two == "b'" and not ident_before(text, i)):
            k = i + (2 if c != "'" else 1)
            char = re.match(r"\\(?:u\{[0-9A-Fa-f_]*\}|x[0-9A-Fa-f]{2}|.)'|[^\\']'", text[k:], re.DOTALL)
            if char:
                empty(k, k + char.end() - 1)
                i = k + char.end()
            else:
                i = k  # a lifetime or a label: code
        else:
            i += 1
    return "".join(out)


def ident_before(text: str, i: int) -> bool:
    """Whether the character before `i` continues an identifier, so that `b`, `c` or `r` at
    `i` ends a name rather than prefixing a literal."""
    return i > 0 and (text[i - 1].isalnum() or text[i - 1] == "_")


def code_lines(path: Path) -> list[str]:
    """A file's lines, its strings, characters and comments blanked across lines."""
    return blank(path.read_text()).split("\n")


def asserts(files: list[Path]) -> list[str]:
    out = []
    for path in files:
        lines = code_lines(path)
        for number, _ in enumerate(production(path), 1):
            code = lines[number - 1]
            if ASSERT.search(code):
                out.append(f"  {path.relative_to(ROOT)}:{number}: assert in production code (Rule 2)")
    return out


def exceptions(crate: Path, files: list[Path]) -> list[str]:
    out = []
    allowed = EXEMPTABLE | (PRINTING if crate.name in APPLICATION_CRATES else set())
    for path in files:
        text = code(path)
        may = unsafe_allowed(path) or caller(path)
        here = allowed | ({UNSAFE_LINT} if may else set())
        for number, part in lint_parts(text):
            where = f"  {path.relative_to(ROOT)}:{number}:"
            if not LINT_NAME.match(part):
                out.append(f"{where} an exception's part neither a lint nor its reason: {part} (Rule 19)")
            elif part.strip() not in here:
                out.append(f"{where} {part.strip()} may not be excepted (Rule 19)")
    return out


EXCEPTION = re.compile(r"\b(?:allow|expect)\s*\(")


def lint_parts(text: str) -> list[tuple[int, str]]:
    """Each part of every `allow(…)` and `expect(…)` in every attribute, directly or under
    `cfg_attr`, however many it holds, its reason left out: by its line, read with its
    brackets matched in the code alone."""
    out = []
    for m in ATTRIBUTE.finditer(text):
        end = closing(text, m.end() - 1, "[", "]")
        for e in EXCEPTION.finditer(text, m.end(), end):
            inside = text[e.end() : closing(text, e.end() - 1, "(", ")") - 1]
            number = line_of(text, e.start())
            for part in arguments(inside):
                if part.strip() and not re.match(r"\s*reason\s*=", part):
                    out.append((number, part.strip()))
    return out


def unsafe_allowed(path: Path) -> bool:
    return str(path.relative_to(ROOT)) in UNSAFE_MODULES


def caller(path: Path) -> bool:
    return str(path.relative_to(ROOT)) in UNSAFE_CALLERS


MAP_CALL = re.compile(r"\bunsafe \{ CacheFile::map\(")


def unsafe_code(crate: Path) -> list[str]:
    out = []
    for path in every_target(crate):
        if unsafe_allowed(path):
            continue
        for number, code in enumerate(code_lines(path), 1):
            if caller(path):
                code = MAP_CALL.sub("", code)
            if UNSAFE.search(code):
                out.append(f"  {path.relative_to(ROOT)}:{number}: unsafe outside UNSAFE_MODULES (Rule 4)")
    return out


def call_graph(files: list[Path]) -> tuple[dict, set, dict]:
    calls, bounded, where = {}, set(), {}
    for path in files:
        current, marked = None, False
        blanked = code_lines(path)
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
                calls[current] |= set(CALL.findall(blanked[number - 1]))
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
    calls, bounded, where = call_graph(sources(crate))
    out = []
    for component in cycles(calls):
        if not bounded & set(component):
            names = ", ".join(f"{n} ({where[n]})" for n in component)
            out.append(f"  recursion without a stated bound (Rule 15): {names}")
    return out


def code(path: Path) -> str:
    """A file's text with its strings, characters and comments blanked, positions kept."""
    return blank(path.read_text())


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
# An error with no type of its own (Rule 8): a string, owned, borrowed, boxed or copied on
# write, or a boxed `dyn Error`, read with its white space removed.
STRING_TYPE = re.compile(
    r"^(?:(?:std|alloc)::string::)?String$|^&(?:'\w+)?str$|^(?:(?:std::)?boxed::)?Box<(?:str|dyn.*)>$"
    r"|^(?:(?:std|alloc)::borrow::)?Cow<(?:'\w+,)?str>$"
)
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
                    out.append(f"  {where}:{line_of(text, m.start())}: an error with no type of its own (Rule 8)")
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
    """Every `.rs` file of the crate git would commit, by `paths.committable`: a directory
    named as an output's but within `src/` is read, and no build output is."""
    return sorted(p for p in committable() if p.suffix == ".rs" and crate in p.parents)


def sources(crate: Path) -> list[Path]:
    """The crate's `src/` files git would commit."""
    return [p for p in every_target(crate) if (crate / "src") in p.parents]


def main() -> int:
    if sys.argv[1:] in (["--errors"], ["--ignores"]):
        found = []
        for crate in crates():
            if sys.argv[1] == "--errors":
                found += errors(crate, sources(crate))
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
        files = sources(crate)
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
