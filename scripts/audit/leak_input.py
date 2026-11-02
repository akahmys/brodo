#!/usr/bin/env python3
"""The text betterleaks reads through standard input, where its default allow-list, which
skips paths such as `node_modules/`, `.svg` and `.gltf` by name, matches nothing
(AUDITING.md §2, ADR-0003).

    leak_input.py --tree      every text file git would commit (`paths.text_files`)
    leak_input.py --staged    every text file staged, as staged
    leak_input.py --history   every text blob of every commit

Each file or blob is written whole, behind a line naming it, `#### NAME`, so that a
finding can be traced; a file that is not text, UTF-8 with no NUL, is left out, as `paths.is_text` leaves
it out of every other scan.
"""

import subprocess
import sys

from paths import ROOT, is_text_bytes, text_files


def git(*args: str, data: bytes = b"") -> bytes:
    return subprocess.run(["git", *args], cwd=ROOT, input=data, capture_output=True,
                          check=True).stdout


def write(name: str, data: bytes) -> None:
    if is_text_bytes(data):
        # A line holding no `=`, `:` or `,`, which a rule would read as an assignment
        # joining the name to the text before it.
        sys.stdout.buffer.write(f"\n\n#### {name}\n".encode() + data)


def main() -> int:
    mode = sys.argv[1:]
    if mode == ["--tree"]:
        for path in text_files():
            write(str(path.relative_to(ROOT)), path.read_bytes())
    elif mode == ["--staged"]:
        names = git("diff", "--cached", "--name-only", "--diff-filter=d", "-z").split(b"\0")
        for name in filter(None, names):
            write(name.decode(), git("show", f":{name.decode()}"))
    elif mode == ["--history"]:
        objects = git("rev-list", "--all", "--objects").decode().splitlines()
        names = "\n".join(o.split(" ")[0] for o in objects).encode()
        # Each object as `<name> <type> <size>`, its bytes and a line end, in one call.
        batch, at = git("cat-file", "--batch", data=names), 0
        while at < len(batch):
            end = batch.index(b"\n", at)
            name, kind, size = batch[at:end].decode().split(" ")
            data = batch[end + 1 : end + 1 + int(size)]
            at = end + 1 + int(size) + 1
            if kind == "blob":
                write(name, data)
    else:
        print(__doc__, file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
