#!/usr/bin/env python3
"""Checks on the workspace manifests and the layout (CODING.md Rules 4, 11, 19, 20, L1, L3 and
L4; TESTING.md V1, V3, V7, V8, T6).

1. Every member sits in `crates/` or is `xtask`, so `spikes/` never joins the build.
2. Every member declares `[lints] workspace = true`. Without it a crate escapes every lint
   in `[workspace.lints]`, `unsafe_code` included, and nothing else notices.
3. Dependencies point only down the layer order (Rule 11), in every dependency kind.
   A member missing from the order fails, so a new crate is placed before it is built.
   The order is in `project.py`.
4. Every member takes `version.workspace = true` and `license.workspace = true` (V1), the
   workspace version is `MAJOR.MINOR.PATCH` with no pre-release or build part (V1),
   and it is the newest released version in CHANGELOG.md, or 0.0.0 while there is none
   (V3). A version whose PATCH is above 0 states the `rust-version` that tag
   `vMAJOR.MINOR.0`'s `Cargo.toml` states (V7); the tag missing fails.
5. Every dependency of a member is inherited from `[workspace.dependencies]` (Rule 20).
6. No cargo configuration in the repository sets `rustflags`, which can switch any lint
   off without a trace in the code (Rule 19). The audit also runs with `RUSTFLAGS` empty,
   which overrides a user-level configuration.
7. Every crate holding a module in `UNTRUSTED_INPUT` is fuzzed (T6): `fuzz/Cargo.toml`
   exists, is its own workspace, so cargo-fuzz never joins the main one, declares a
   target, and depends on the crate by path. Whether each way in has a target, and whether the targets run, is
   not checked.
8. Every entry at the root is named in ARCHITECTURE.md §3's tree (L3), and every script, a
   `.py` or `.sh` file or an executable one, lies under `SCRIPT_HOMES` (L4). The files read
   are those git tracks or would add, so a new one is checked before it is staged.
9. What is published (V8): the workspace publishes to crates.io alone, every crate takes
   `publish.workspace = true`, the workspace's dependency on brodo states the workspace
   version as well as its path, and each crate's package, as `cargo package --list`
   gives it, holds its `Cargo.toml`, README.md and both licences, the licences the root's
   bytes, and nothing beyond them, `src/` and `examples/`.
"""

import json
import os
import re
import subprocess
import sys
from pathlib import Path

from paths import committable
from project import LAYERS, TOOLS, UNTRUSTED_INPUT, crate_of

ROOT = Path(__file__).resolve().parent.parent.parent

VERSION_INHERITED = re.compile(r"^version\.workspace\s*=\s*true\s*$", re.MULTILINE)
LICENSE_INHERITED = re.compile(r"^license\.workspace\s*=\s*true\s*$", re.MULTILINE)
WORKSPACE_VERSION = re.compile(r'^version\s*=\s*"([^"]+)"', re.MULTILINE)
RELEASE = re.compile(r"^## (\d+\.\d+\.\d+) ", re.MULTILINE)
VERSION_FORM = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")
RUST_VERSION = re.compile(r'^rust-version\s*=\s*"([^"]+)"', re.MULTILINE)
DEP_SECTION = re.compile(r"^\[(?:target\.[^\]]+\.)?(?:dev-|build-)?dependencies\]\s*$")
DEP_SUBTABLE = re.compile(r"^\[(?:target\.[^\]]+\.)?(?:dev-|build-)?dependencies\.([\w-]+)\]\s*$")
DEP_LINE = re.compile(r"^([\w-]+)(\.workspace)?\s*=\s*(.*)$")
PUBLISH_INHERITED = re.compile(r"^publish\.workspace\s*=\s*true\s*$", re.MULTILINE)
PUBLISH_CRATES_IO = re.compile(r'^publish\s*=\s*\["crates-io"\]\s*$', re.MULTILINE)
BRODO_DEPENDENCY = re.compile(r"^brodo\s*=\s*\{([^}\n]*)\}", re.MULTILINE)
DEP_VERSION = re.compile(r'\bversion\s*=\s*"([^"]+)"')
LICENCES = ("LICENSE-APACHE", "LICENSE-MIT")
# What a package holds besides `src/` and `examples/` (V8); cargo writes the last three.
PACKAGE_FILES = {"Cargo.toml", "README.md", *LICENCES, "Cargo.toml.orig", "Cargo.lock", ".cargo_vcs_info.json"}
PACKAGE_DIRS = ("src/", "examples/")
LINTS = re.compile(r"^\[lints\]\s*\nworkspace\s*=\s*true\s*$", re.MULTILINE)
LAYOUT_TREE = re.compile(r"^## 3\. Repository layout\n.*?^```\n(.*?)^```", re.DOTALL | re.MULTILINE)
# Where a script may lie (L4); a spike's own code is the spike's (PLANNING.md §3).
SCRIPT_HOMES = ("scripts/audit/", "scripts/hooks/", "xtask/", "spikes/")


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


def version_rules() -> list[str]:
    """The version's form (V1), and `rust-version` unchanged within a MINOR (V7)."""
    text = (ROOT / "Cargo.toml").read_text()
    stated = WORKSPACE_VERSION.search(text)
    version = stated.group(1) if stated else "none"
    form = VERSION_FORM.match(version)
    if not form:
        return [f"  Cargo.toml version {version} is not MAJOR.MINOR.PATCH (V1)"]
    major, minor, patch = form.groups()
    if patch == "0":
        return []
    tag = f"v{major}.{minor}.0"
    shown = subprocess.run(
        ["git", "show", f"refs/tags/{tag}:Cargo.toml"], cwd=ROOT, capture_output=True, text=True
    )
    if shown.returncode != 0:
        return [f"  version {version} is a PATCH, and tag {tag} is not here to compare its rust-version with (V7)"]
    now, then = RUST_VERSION.search(text), RUST_VERSION.search(shown.stdout)
    now, then = (now.group(1) if now else "none"), (then.group(1) if then else "none")
    if now != then:
        return [f"  version {version} is a PATCH, and its rust-version {now} is not {tag}'s {then} (V7)"]
    return []


def publishing(packages: list[dict]) -> list[str]:
    """Where and what each crate publishes (V8)."""
    text = (ROOT / "Cargo.toml").read_text()
    out = []
    if not PUBLISH_CRATES_IO.search(text):
        out.append('  Cargo.toml does not set `publish = ["crates-io"]` for the workspace (V8)')
    stated = WORKSPACE_VERSION.search(text)
    dep = BRODO_DEPENDENCY.search(text)
    version = DEP_VERSION.search(dep.group(1)) if dep else None
    if not version or not stated or version.group(1) != stated.group(1):
        found = version.group(1) if version else "none"
        workspace = stated.group(1) if stated else "none"
        out.append(f"  the workspace's dependency on brodo states version {found}, not the workspace's {workspace} (V8)")
    for pkg in packages:
        name = pkg["name"]
        if name in TOOLS:
            continue
        manifest = Path(pkg["manifest_path"])
        if not PUBLISH_INHERITED.search(manifest.read_text()):
            out.append(f"  {name}: does not declare `publish.workspace = true` (V8)")
        for licence in LICENCES:
            held = manifest.parent / licence
            if not held.is_file() or held.read_bytes() != (ROOT / licence).read_bytes():
                out.append(f"  {name}: {licence} is not the root's (V8)")
        listed = subprocess.run(
            ["cargo", "package", "--list", "--allow-dirty", "-p", name],
            cwd=ROOT, capture_output=True, text=True,
        )
        if listed.returncode != 0:
            out.append(f"  {name}: `cargo package --list` failed: {listed.stderr.strip()} (V8)")
            continue
        held = set(listed.stdout.split())
        for missing in sorted({"Cargo.toml", "README.md", *LICENCES} - held):
            out.append(f"  {name}: its package does not hold {missing} (V8)")
        for extra in sorted(f for f in held if f not in PACKAGE_FILES and not f.startswith(PACKAGE_DIRS)):
            out.append(f"  {name}: its package holds {extra} (V8)")
    return out


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


def files() -> list[str]:
    """The files git would commit, by `paths.committable`, relative to the root."""
    return [str(p.relative_to(ROOT)) for p in committable()]


def layout() -> list[str]:
    tree = LAYOUT_TREE.search((ROOT / "ARCHITECTURE.md").read_text())
    if not tree:
        return ["  ARCHITECTURE.md §3 has no layout tree (L3)"]
    named = set()
    for line in tree.group(1).splitlines():
        text = re.sub(r"[│├└─]", " ", line.split("#")[0])
        for token in re.split(r"[,\s]+", text):
            if token and "(" not in token and ")" not in token:
                named.add(token.split("/")[0])
    out = []
    for root in sorted({f.split("/")[0] for f in files()} - named):
        out.append(f"  {root} is at the root and not in ARCHITECTURE.md §3 (L3)")
    for f in files():
        script = f.endswith((".py", ".sh")) or os.access(ROOT / f, os.X_OK)
        if script and not f.startswith(SCRIPT_HOMES):
            out.append(f"  {f} is a script outside {', '.join(SCRIPT_HOMES)} (L4)")
    return out


def main() -> int:
    packages = metadata()["packages"]
    failures = release_table() + version_rules() + cargo_config() + fuzzer() + layout()
    failures += publishing(packages)
    for pkg in packages:
        failures += check_package(pkg)
    if failures:
        print("\n".join(failures))
        return 1
    print("  PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
