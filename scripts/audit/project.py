"""The project's own settings for the audit: the one file to edit when the crates change.

A crate is named by its package name, and its directory under `crates/` has the same name.
Changing this file is changing a rule (AGENTS.md, "Changing the rules").
"""

# CODING.md Rule 11. A crate may depend only on crates earlier in this list.
LAYERS = [
    "brodo",
    "colino",
]

# Tooling may depend on anything, and nothing may depend on it (Rule 11).
TOOLS = ["xtask"]

# Application crates: they may print (Rule 18) and use `anyhow` or `String` errors (Rule 8).
# `xtask` is always one.
APPLICATIONS = []

# Modules that parse untrusted bytes (a file, a mesh), as directories or files under
# `crates/<crate>/src/`. Indexing and slicing are banned in them (Rule 3), and their crate
# is fuzzed (TESTING.md T6). A module is listed in the commit that creates it (ADR-0009).
UNTRUSTED_INPUT = ["crates/brodo/src/cache.rs", "crates/brodo/src/view.rs"]

# The one file allowed to contain `unsafe` (Rule 4, ADR-0008): brodo's memory-mapping
# module, once it exists. At most one entry.
UNSAFE_MODULES = ["crates/brodo/src/mapping.rs"]

# The targets the project is built for (Rule 12). Adding one is a claim that it is.
TARGETS = ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"]


def crate_of(path: str) -> str:
    """The crate a path under `crates/<crate>/src/` belongs to."""
    return path.split("/")[1]


if __name__ == "__main__":
    import sys

    # `python3 scripts/audit/project.py NAME` prints a setting space-separated, for the shell.
    print(" ".join(globals()[sys.argv[1]]))
