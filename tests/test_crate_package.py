#!/usr/bin/env python
"""Every file the code compiles in -- `include_bytes!`, `include_str!`,
`include_wgsl!` -- is in the crate's package, which `Cargo.toml`'s `include`
list draws. What is not fails `cargo publish` on the tag, after PyPI took the
version, and leaves PyPI's source package unable to build: maturin takes its
files from the same list. v0.5.11 went out that way, without
`res/colormaps.bin` and `res/examples-shipped.txt`.

    python tests/test_crate_package.py

Add the path to `include` in `Cargo.toml` when this fails.
"""

import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
# A path given as a literal, relative to the file it is in, or through
# `concat!(env!("CARGO_MANIFEST_DIR"), "/...")`, relative to the root.
RELATIVE = re.compile(r'include_(?:bytes|str|wgsl)!\(\s*"([^"]+)"\s*\)')
FROM_ROOT = re.compile(r'include_(?:bytes|str|wgsl)!\(\s*concat!\(\s*env!\("CARGO_MANIFEST_DIR"\)\s*,\s*"([^"]+)"\s*\)\s*\)')


def compiled_in() -> dict[str, str]:
    """Every file compiled in, relative to the root, and where."""
    found = {}
    for source in (ROOT / "src").rglob("*.rs"):
        text = source.read_text()
        for n, line in enumerate(text.splitlines(), 1):
            if line.lstrip().startswith("//"):
                continue
            where = f"{source.relative_to(ROOT)}:{n}"
            for m in RELATIVE.finditer(line):
                path = (source.parent / m.group(1)).resolve().relative_to(ROOT)
                found[path.as_posix()] = where
            for m in FROM_ROOT.finditer(line):
                found[m.group(1).lstrip("/")] = where
    return found


def packaged() -> set[str]:
    listed = subprocess.run(
        ["cargo", "package", "--list", "--allow-dirty", "--quiet"],
        cwd=ROOT, check=True, capture_output=True, text=True,
    ).stdout
    return {line.strip() for line in listed.splitlines() if line.strip()}


def test_what_is_compiled_in_is_packaged():
    wanted = compiled_in()
    assert len(wanted) > 20, f"found only {len(wanted)} files compiled in: the patterns no longer match"
    missing = {path: where for path, where in wanted.items() if path not in packaged()}
    assert not missing, "not in the crate's package, add them to `include` in Cargo.toml: " + ", ".join(
        f"{path} ({where})" for path, where in sorted(missing.items())
    )


if __name__ == "__main__":
    test_what_is_compiled_in_is_packaged()
    print("ok")
    sys.exit(0)
