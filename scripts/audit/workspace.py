#!/usr/bin/env python3
"""Checks on the workspace manifests (CODING.md Rules 4, 11, 19, 20 and L1; TESTING.md V1, V3, T6).

1. Every member sits in `crates/` or is `xtask`, so `spikes/` never joins the build.
2. Every member declares `[lints] workspace = true`. Without it a crate escapes every lint
   in `[workspace.lints]`, `unsafe_code` included, and nothing else notices.
3. Dependencies point only down the layer order (Rule 11), in every dependency kind.
   A member missing from the order fails, so a new crate is placed before it is built.
   The order is in `project.py`.
4. Every member takes `version.workspace = true` and `license.workspace = true` (V1), and the workspace version is the
   newest released version in CHANGELOG.md, or 0.0.0 while there is none (V3).
5. Every dependency of a member is inherited from `[workspace.dependencies]` (Rule 20).
6. No cargo configuration in the repository sets `rustflags`, which can switch any lint
   off without a trace in the code (Rule 19). The audit also runs with `RUSTFLAGS` empty,
   which overrides a user-level configuration.
7. Every crate holding a module in `UNTRUSTED_INPUT` is fuzzed (T6): `fuzz/Cargo.toml`
   exists, is its own workspace, so cargo-fuzz never joins the main one, declares a
   target, and depends on the crate by path. Whether each way in has a target, and whether the targets run, is
   not checked.
"""

import json
import re
import subprocess
import sys
from pathlib import Path

from project import LAYERS, TOOLS, UNTRUSTED_INPUT, crate_of

ROOT = Path(__file__).resolve().parent.parent.parent

VERSION_INHERITED = re.compile(r"^version\.workspace\s*=\s*true\s*$", re.MULTILINE)
LICENSE_INHERITED = re.compile(r"^license\.workspace\s*=\s*true\s*$", re.MULTILINE)
WORKSPACE_VERSION = re.compile(r'^version\s*=\s*"([^"]+)"', re.MULTILINE)
RELEASE = re.compile(r"^## (\d+\.\d+\.\d+) ", re.MULTILINE)
DEP_SECTION = re.compile(r"^\[(?:target\.[^\]]+\.)?(?:dev-|build-)?dependencies\]\s*$")
DEP_SUBTABLE = re.compile(r"^\[(?:target\.[^\]]+\.)?(?:dev-|build-)?dependencies\.([\w-]+)\]\s*$")
DEP_LINE = re.compile(r"^([\w-]+)(\.workspace)?\s*=\s*(.*)$")
LINTS = re.compile(r"^\[lints\]\s*\nworkspace\s*=\s*true\s*$", re.MULTILINE)


def metadata() -> dict:
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"],
        cwd=ROOT, capture_output=True, text=True, check=True,
    )
    return json.loads(out.stdout)


def check_package(pkg: dict) -> list[str]:
    out = []
    name = pkg["name"]
    manifest = Path(pkg["manifest_path"])
    rel = manifest.parent.relative_to(ROOT)

    if not (rel.parts[0] == "crates" or name in TOOLS):
        out.append(f"  {name}: member at {rel} is outside crates/ (L1)")

    # The system python may predate tomllib, so the section is matched as text.
    text = manifest.read_text()
    if not LINTS.search(text) or re.search(r"^\[lints\.", text, re.MULTILINE):
        out.append(f"  {name}: does not declare `[lints] workspace = true` (Rule 4)")

    out += dependencies(name, text)
    if not VERSION_INHERITED.search(text):
        out.append(f"  {name}: does not declare `version.workspace = true` (V1)")
    if not LICENSE_INHERITED.search(text):
        out.append(f"  {name}: does not declare `license.workspace = true` (V1)")

    if name in TOOLS:
        return out
    if name not in LAYERS:
        out.append(f"  {name}: not in the layer order in project.py (Rule 11)")
        return out
    rank = LAYERS.index(name)
    for dep in pkg["dependencies"]:
        target = dep["name"]
        kind = dep["kind"] or "normal"
        if target in TOOLS:
            out.append(f"  {name} -> {target} ({kind}): nothing depends on tooling (Rule 11)")
        elif target in LAYERS and LAYERS.index(target) >= rank:
            out.append(f"  {name} -> {target} ({kind}): points up the layer order (Rule 11)")
    return out


def dependencies(name: str, text: str) -> list[str]:
    out, in_section, subtable = [], False, None
    for line in text.split("\n"):
        stripped = line.split("#")[0].strip()
        if stripped.startswith("["):
            in_section = bool(DEP_SECTION.match(stripped))
            match = DEP_SUBTABLE.match(stripped)
            if subtable:
                out.append(f"  {name}: dependency {subtable} is not inherited from the workspace (Rule 20)")
            subtable = match.group(1) if match else None
            continue
        if subtable and re.match(r"^workspace\s*=\s*true$", stripped):
            subtable = None
        elif in_section and stripped:
            match = DEP_LINE.match(stripped)
            inherited = match and (match.group(2) or re.search(r"\bworkspace\s*=\s*true\b", match.group(3)))
            if not inherited:
                dep = match.group(1) if match else stripped
                out.append(f"  {name}: dependency {dep} is not inherited from the workspace (Rule 20)")
    if subtable:
        out.append(f"  {name}: dependency {subtable} is not inherited from the workspace (Rule 20)")
    return out


def cargo_config() -> list[str]:
    out = []
    for path in [ROOT / ".cargo" / "config", ROOT / ".cargo" / "config.toml"]:
        if path.is_file() and "rustflags" in path.read_text():
            out.append(f"  {path.relative_to(ROOT)} sets rustflags (Rule 19)")
    return out


def release_table() -> list[str]:
    stated = WORKSPACE_VERSION.search((ROOT / "Cargo.toml").read_text())
    releases = RELEASE.findall((ROOT / "CHANGELOG.md").read_text())
    expected = releases[0] if releases else "0.0.0"
    if not stated or stated.group(1) != expected:
        found = stated.group(1) if stated else "none"
        return [f"  Cargo.toml version {found}, CHANGELOG.md's newest release is {expected} (V3)"]
    return []


def fuzzer() -> list[str]:
    fuzzed = sorted({crate_of(module) for module in UNTRUSTED_INPUT})
    if not fuzzed:
        return []
    manifest = ROOT / "fuzz" / "Cargo.toml"
    if not manifest.is_file():
        return [f"  fuzz/Cargo.toml is missing, and UNTRUSTED_INPUT lists {', '.join(UNTRUSTED_INPUT)} (T6)"]
    text = manifest.read_text()
    out = []
    if not re.search(r"^\[workspace\]", text, re.MULTILINE):
        out.append("  fuzz/Cargo.toml has no [workspace] table of its own (T6)")
    if not re.search(r"^\[\[bin\]\]", text, re.MULTILINE):
        out.append("  fuzz/Cargo.toml declares no target (T6)")
    for crate in fuzzed:
        name, path = re.escape(crate), re.escape(f"../crates/{crate}")
        inline = rf'^{name}\s*=\s*\{{[^}}\n]*path\s*=\s*"{path}"'
        table = rf'^\[dependencies\.{name}\]\s*\n(?:[^\[\n].*\n)*?path\s*=\s*"{path}"'
        if not re.search(f"{inline}|{table}", text, re.MULTILINE):
            out.append(f"  {crate} parses untrusted input and fuzz/Cargo.toml does not depend on it (T6)")
    return out


def main() -> int:
    failures = release_table() + cargo_config() + fuzzer()
    for pkg in metadata()["packages"]:
        failures += check_package(pkg)
    if failures:
        print("\n".join(failures))
        return 1
    print("  PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
