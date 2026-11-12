#!/usr/bin/env python3
"""The writing rules in AGENTS.md that a script can check.

1. Language: no Japanese script in the repository's text files (AGENTS.md writing rule 6).
2. Tense: `ARCHITECTURE.md` and `AGENTS.md` say what is true now. A line carrying both a
   date and a past-tense verb is the shape of a dated self-correction (writing rule 2).
3. Links: every relative Markdown link in the documents and in source comments, and every
   relative `src`, `srcset` and `href` of HTML in a document, resolves (writing rule 1).
4. The ADR index: no duplicate or missing number, the index and the directory agree in
   both directions, and every record carries a Status and a Date.
5. No CLAUDE.md: Claude Code reads AGENTS.md only while none of these exists.
6. Sections: every `NAME.md §N` or `NAME.md §N.M` naming a document at the root, in the
   documents and in source comments, names a heading numbered so in it (writing rule 1).
7. Provenance (AGENTS.md S3): every source file under `crates/*/src/` has a row in
   PROVENANCE.md, and every row names a file that exists. Whether the row is true,
   nothing checks.
8. Crate READMEs (AGENTS.md): every crate in `LAYERS` has a README.md, read in as its
   documentation's front page by `#![doc = include_str!("../README.md")]` in its `lib.rs`,
   so that its code runs as a doctest. What it says, nothing checks.
9. Hidden items (AGENTS.md): an item under `#[doc(hidden)]` promises nothing, so no other
   file's doc comment under `crates/*/src/`, no crate README, ARCHITECTURE.md and
   docs/formats.md name it in backquotes, alone or at the end of a path. A hidden module is
   not held to this, its own files naming what is in it.
10. Retrospectives (PLANNING.md §7): every milestone in ROADMAP.md §2, a `### P<n>` heading,
   lists `The retrospective and refactoring (PLANNING.md §7)` in its checklist, and does not
   tick its `Version` item while that one is open. Whether the unit was done well, nothing.
"""

import re
import sys
from pathlib import Path
from typing import Optional

from paths import JAPANESE, text_files
from project import LAYERS, RULE_FILES

ROOT = Path(__file__).resolve().parent.parent.parent

PRESENT_TENSE = ["ARCHITECTURE.md", "AGENTS.md"]
DATE = re.compile(r"\b20\d\d-\d\d-\d\d\b")
PAST = re.compile(r"\b(was|were|had|used to|became|no longer|turned out)\b")
# `NAME.md §N`, a root document's or `docs/`'s, its name in a link or in backquotes or bare.
SECTION = re.compile(r"\b((?:docs/)?[A-Za-z][A-Za-z_]*\.md)[)`]? §(\d+(?:\.\d+)*)")
LINK = re.compile(r"\]\(([^)#:\s][^)\s#]*)(#[^)]*)?\)")
# HTML in a document: `<img src="…">`, `<source srcset="… 2x, …">`, `<a href="…">`.
HTML_LINK = re.compile(r'\b(src|srcset|href)\s*=\s*"([^"]*)"')


def walk() -> list[Path]:
    """The text files git would commit (paths.py)."""
    return text_files()


# A numbered heading: `#`s, its number, a dot or not, and a space, so that `4.2` is not
# found in `4.2.1`.
HEADING = re.compile(r"^(#+) (\d+(?:\.\d+)*)\.? ", re.MULTILINE)


def section(home: str, number: str) -> Optional[str]:
    """The text of section `number` of document `home`, to the next heading as high, or
    nothing if it has none so numbered: the one reading of a section by its number."""
    path = ROOT / home
    if not path.is_file():
        return None
    lines = path.read_text().splitlines()
    for i, line in enumerate(lines):
        heading = HEADING.match(line)
        if heading and heading.group(2) == number:
            depth = len(heading.group(1))
            rest = lines[i + 1:]
            ends = [j for j, l in enumerate(rest) if re.match(r"^#{1,%d} " % depth, l)]
            return "\n".join(rest[:ends[0]] if ends else rest)
    return None


def language(paths: list[Path]) -> list[str]:
    out = []
    for path in paths:
        for number, line in enumerate(path.read_text(errors="replace").split("\n"), 1):
            if JAPANESE.search(line):
                out.append(f"  {path.relative_to(ROOT)}:{number}: Japanese text")
                break
    return out


def tense() -> list[str]:
    out = []
    for name in PRESENT_TENSE:
        fence = False
        for number, line in enumerate((ROOT / name).read_text().split("\n"), 1):
            if line.strip().startswith("```"):
                fence = not fence
            elif not fence and DATE.search(line) and PAST.search(line):
                out.append(f"  {name}:{number}: dated past tense: {line.strip()[:80]}")
    return out


def links(paths: list[Path]) -> list[str]:
    out = []
    for path in paths:
        if path.suffix not in {".md", ".rs"}:
            continue
        text = path.read_text(errors="replace")
        targets = [m.group(1) for m in LINK.finditer(text)]
        if path.suffix == ".md":
            for m in HTML_LINK.finditer(text):
                urls = m.group(2).split(",") if m.group(1) == "srcset" else [m.group(2)]
                targets += [u.split()[0] for u in urls if u.split()]
        for target in targets:
            target = target.split("#")[0]
            if not target or ":" in target:
                continue
            if not (path.parent / target).exists():
                out.append(f"  {path.relative_to(ROOT)}: {target} does not exist")
    return out


def sections(paths: list[Path]) -> list[str]:
    headings = {}
    for doc in [*ROOT.glob("*.md"), *(ROOT / "docs").glob("*.md")]:
        found = {m.group(2) for m in HEADING.finditer(doc.read_text())}
        headings[doc.name] = found
        headings[str(doc.relative_to(ROOT))] = found
    out = []
    for path in paths:
        if path.suffix not in {".md", ".rs"}:
            continue
        for number, line in enumerate(path.read_text(errors="replace").split("\n"), 1):
            for name, section in SECTION.findall(line):
                if name in headings and section not in headings[name]:
                    out.append(f"  {path.relative_to(ROOT)}:{number}: {name} has no §{section}")
    return out


def adr_index() -> list[str]:
    out = []
    directory = ROOT / "docs" / "adr"
    files = sorted(f for f in directory.glob("*.md") if f.name != "README.md")
    numbers = [int(f.name[:4]) for f in files]
    for number in sorted(set(numbers)):
        if numbers.count(number) > 1:
            out.append(f"  ADR-{number:04d} is used by more than one file")
    for number in range(1, max(numbers, default=0) + 1):
        if number not in numbers:
            out.append(f"  ADR-{number:04d} is missing; the numbering has a gap")
    readme = (directory / "README.md").read_text()
    indexed = set(re.findall(r"\]\((\d{4}-[a-z0-9-]+\.md)\)", readme))
    on_disk = {f.name for f in files}
    out += [f"  the index names {n}, which is not there" for n in sorted(indexed - on_disk)]
    out += [f"  {n} is not in the index" for n in sorted(on_disk - indexed)]
    for f in files:
        head = f.read_text().split("\n## ")[0]
        for field in ("**Status**", "**Date**"):
            if field not in head:
                out.append(f"  {f.name} has no {field}")
    return out


def rule_files() -> list[str]:
    """Each path AGENTS.md's "Changing the rules" names in backquotes is one `project.py`'s
    RULE_FILES holds, so that the hook holds a commit to the list the rule states. A path
    ending in `/` is a directory, held by a file in it."""
    text = (ROOT / "AGENTS.md").read_text()
    start = text.find("## Changing the rules")
    paragraph = text[start:].split("\n\n")[1] if start >= 0 else ""
    pattern = re.compile(RULE_FILES)
    out = []
    for path in re.findall(r"`([^`]+)`", paragraph):
        if path == "Cargo.toml":
            continue  # its lint levels, read from its diff
        probe = path + "x" if path.endswith("/") else path
        if not pattern.search(probe):
            out.append(f"  AGENTS.md names {path} as a rule, which RULE_FILES does not hold")
    return out if paragraph else ["  AGENTS.md has no paragraph under 'Changing the rules'"]


def instruction_files() -> list[str]:
    names = ["CLAUDE.md", ".claude/CLAUDE.md", "CLAUDE.local.md"]
    return [f"  {n} exists; Claude Code would stop reading AGENTS.md" for n in names
            if (ROOT / n).exists()]


def provenance() -> list[str]:
    recorded = set(re.findall(r"^\|\s*`(crates/[^`]+\.rs)`", (ROOT / "PROVENANCE.md").read_text(),
                              re.MULTILINE))
    sources = {str(f.relative_to(ROOT)) for f in (ROOT / "crates").glob("*/src/**/*.rs")}
    out = [f"  {n} has no row in PROVENANCE.md" for n in sorted(sources - recorded)]
    out += [f"  PROVENANCE.md names {n}, which is not there" for n in sorted(recorded - sources)]
    return out


READ_IN = '#![doc = include_str!("../README.md")]'


def readmes() -> list[str]:
    out = []
    for crate in LAYERS:
        directory = ROOT / "crates" / crate
        if not (directory / "README.md").is_file():
            out.append(f"  crates/{crate} has no README.md")
        lib = directory / "src" / "lib.rs"
        if not lib.is_file() or READ_IN not in lib.read_text():
            out.append(f"  crates/{crate}/src/lib.rs does not read its README in: {READ_IN}")
    return out


# An item under `#[doc(hidden)]`, attributes and doc lines between them: its name.
HIDDEN = re.compile(
    r"#\[doc\(hidden\)\]\s*(?:(?:#\[[^\]]*\]|///[^\n]*)\s*)*"
    r"pub(?:\([^)]*\))?\s+(?:const\s+|unsafe\s+)*(?:fn|struct|enum|trait|type|const|static)\s+(\w+)"
)
DOC_LINE = re.compile(r"^\s*//[/!](.*)$", re.MULTILINE)
PROMISES = ["ARCHITECTURE.md", "docs/formats.md"]


def hidden() -> list[str]:
    sources = sorted((ROOT / "crates").glob("*/src/**/*.rs"))
    names = {(m[1], f) for f in sources for m in HIDDEN.finditer(f.read_text())}
    texts = [(f, "\n".join(DOC_LINE.findall(f.read_text()))) for f in sources]
    texts += [(f, f.read_text()) for f in sorted((ROOT / "crates").glob("*/README.md"))]
    texts += [(ROOT / p, (ROOT / p).read_text()) for p in PROMISES]
    out = []
    for name, home in sorted(names):
        named = re.compile(r"`(?:[\w:]*::)?" + re.escape(name) + r"(?:\(\))?`")
        out += [f"  {f.relative_to(ROOT)} names `{name}`, hidden in {home.relative_to(ROOT)}"
                for f, text in texts if f != home and named.search(text)]
    return out


RETROSPECTIVE = "The retrospective and refactoring (PLANNING.md §7)"
MILESTONE = re.compile(r"^### (P\d+)\b", re.MULTILINE)
ITEM = re.compile(r"^- \[([ x])\] (.*)$", re.MULTILINE)


def retrospectives() -> list[str]:
    text = (ROOT / "ROADMAP.md").read_text()
    start, end = text.find("\n## 2."), text.find("\n## 3.")
    if start < 0 or end < start:
        return ["  ROADMAP.md has no §2 before its §3"]
    milestones = text[start:end]
    heads = list(MILESTONE.finditer(milestones))
    out = []
    for k, head in enumerate(heads):
        body = milestones[head.end():heads[k + 1].start() if k + 1 < len(heads) else None]
        body = body.split("\n### ")[0]
        items = ITEM.findall(body)
        retro = [ticked for ticked, line in items if line.startswith(RETROSPECTIVE)]
        version = [ticked for ticked, line in items if line.startswith("Version ")]
        if not retro:
            out.append(f"  ROADMAP.md's {head[1]} does not list `{RETROSPECTIVE}`")
        elif "x" in version and " " in retro:
            out.append(f"  ROADMAP.md's {head[1]} ticks its version before its retrospective")
    return out


def main() -> int:
    paths = walk()
    failures = []
    checks = (
        ("language", lambda: language(paths)),
        ("tense", tense),
        ("links", lambda: links(paths)),
        ("sections", lambda: sections(paths)),
        ("ADR index", adr_index),
        ("rule files", rule_files),
        ("instruction files", instruction_files),
        ("provenance", provenance),
        ("crate READMEs", readmes),
        ("hidden items", hidden),
        ("retrospectives", retrospectives),
    )
    for label, check in checks:
        found = check()
        if found:
            failures.append(f"  FAIL ({label}):")
            failures.extend(found)
    if failures:
        print("\n".join(failures))
        return 1
    print("  PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
