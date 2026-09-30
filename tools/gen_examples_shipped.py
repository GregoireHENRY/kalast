#!/usr/bin/env python
"""Write `res/examples-shipped.txt`: every example file kalast has shipped, by
its path and a fingerprint of its text. A bundle reads it, compiled in, when a
new version's examples replace the old (`install_examples` in
`src/app/update.rs`), to tell an example as kalast shipped it from one a user
changed or added -- which is kept, in `scripts/`.

    python tools/gen_examples_shipped.py      # after changing an example
    python tests/test_examples_shipped.py     # checks it holds them all

From every release tag and the working tree, and `examples/old/` too, since
the bundle ships it. Nothing is ever taken out: a line is a file some bundle
carried, and a beta's tag moving on, or the example changing since, is no
reason for a later version to stop knowing it.

A line: the fingerprint, 16 hex digits, and the path under `examples/`, with
`/`. The fingerprint is FNV-1a, 64 bits, of the file with each `\\r\\n` read as
`\\n`: Windows' checkout writes the one, the tags hold the other.
"""

import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
LIST = ROOT / "res" / "examples-shipped.txt"
HEADER = "# Every example file kalast has shipped: tools/gen_examples_shipped.py\n"


def fingerprint(data: bytes) -> str:
    """FNV-1a, 64 bits, of `data` with each `\\r\\n` as `\\n` --
    `example_fingerprint` in `src/app/update.rs`."""
    h = 0xCBF29CE484222325
    for b in data.replace(b"\r\n", b"\n"):
        h = ((h ^ b) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}"


def git(*args: str) -> bytes:
    return subprocess.run(["git", *args], cwd=ROOT, check=True, capture_output=True).stdout


def tags() -> list[str]:
    return git("tag", "--list", "v*").decode().split()


def tagged() -> set[tuple[str, str]]:
    """(fingerprint, path) of every file under `examples/` at every tag,
    each distinct blob read once."""
    paths: dict[str, set[str]] = {}
    for tag in tags():
        for entry in git("ls-tree", "-r", "-z", "--full-tree", tag, "--", "examples").split(b"\0"):
            if not entry:
                continue
            meta, path = entry.split(b"\t", 1)
            _, kind, blob = meta.split()
            if kind == b"blob":
                paths.setdefault(blob.decode(), set()).add(path.decode().removeprefix("examples/"))
    out = set()
    for blob, names in paths.items():
        value = fingerprint(git("cat-file", "blob", blob))
        out.update((value, name) for name in names)
    return out


def working_tree() -> set[tuple[str, str]]:
    """(fingerprint, path) of the files under `examples/` now: tracked, and
    new ones git does not ignore -- what a bundle built from here carries."""
    out = set()
    listed = git("ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", "examples")
    for name in listed.decode().split("\0"):
        path = ROOT / name
        if name and path.is_file():
            out.add((fingerprint(path.read_bytes()), name.removeprefix("examples/")))
    return out


def read() -> set[tuple[str, str]]:
    if not LIST.exists():
        return set()
    lines = (l for l in LIST.read_text().splitlines() if l and not l.startswith("#"))
    return {tuple(l.split(" ", 1)) for l in lines}


def encode(entries: set[tuple[str, str]]) -> str:
    return HEADER + "".join(f"{p} {name}\n" for p, name in sorted(entries, key=lambda e: (e[1], e[0])))


def main() -> int:
    before = read()
    entries = before | tagged() | working_tree()
    LIST.write_text(encode(entries))
    print(f"wrote {LIST.relative_to(ROOT)}: {len(entries)} files, {len(entries - before)} new, from {len(tags())} tags and the working tree")
    return 0


if __name__ == "__main__":
    sys.exit(main())
