#!/usr/bin/env python3
"""The writing rules in AGENTS.md that a script can check.

1. Language: no Japanese script in the repository's text files (AGENTS.md writing rule 6).
2. Tense: `ARCHITECTURE.md` and `AGENTS.md` say what is true now. A line carrying both a
   date and a past-tense verb is the shape of a dated self-correction (writing rule 2).
3. Links: every relative Markdown link in the documents and in source comments resolves
   (writing rule 1).
4. The ADR index: no duplicate or missing number, the index and the directory agree in
   both directions, and every record carries a Status and a Date.
5. No CLAUDE.md: Claude Code reads AGENTS.md only while none of these exists.
6. Provenance (AGENTS.md S3): every source file under `crates/*/src/` has a row in
   PROVENANCE.md, and every row names a file that exists. Whether the row is true,
   nothing checks.
"""

import os
import re
import sys
from pathlib import Path

from project import OUTPUT_DIRS

ROOT = Path(__file__).resolve().parent.parent.parent
SKIP_DIRS = {".git", *OUTPUT_DIRS}
TEXT = {".md", ".rs", ".toml", ".py", ".sh", ".txt", ".json", ".yml", ".yaml"}

PRESENT_TENSE = ["ARCHITECTURE.md", "AGENTS.md"]
DATE = re.compile(r"\b20\d\d-\d\d-\d\d\b")
PAST = re.compile(r"\b(was|were|had|used to|became|no longer|turned out)\b")
JAPANESE = re.compile("[\u3040-\u30ff\u3400-\u4dbf\u4e00-\u9fff\uff66-\uff9f]")
LINK = re.compile(r"\]\(([^)#:\s][^)\s#]*)(#[^)]*)?\)")


def walk() -> list[Path]:
    out = []
    for directory, subdirs, names in os.walk(ROOT):
        subdirs[:] = [d for d in subdirs if d not in SKIP_DIRS]
        out += [Path(directory) / n for n in names if Path(n).suffix in TEXT]
    return out


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
        for match in LINK.finditer(path.read_text(errors="replace")):
            if ":" in match.group(1):
                continue
            if not (path.parent / match.group(1)).exists():
                out.append(f"  {path.relative_to(ROOT)}: {match.group(1)} does not exist")
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


def main() -> int:
    paths = walk()
    failures = []
    checks = (
        ("language", lambda: language(paths)),
        ("tense", tense),
        ("links", lambda: links(paths)),
        ("ADR index", adr_index),
        ("instruction files", instruction_files),
        ("provenance", provenance),
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
