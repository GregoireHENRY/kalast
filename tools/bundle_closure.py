#!/usr/bin/env python
"""Re-derive the *import closure* half of `tools/bundle-requirements.txt`.

A release bundle carries its own interpreter, and what has to be installed in
it is not `pyproject.toml`'s dependency list -- that is what a *developer*
wants available, and it comes to 766 MB. What the bundle needs is the closure
reached by executing `kalast/__init__.py`, which is smaller and is not written
down anywhere in the source.

So it is measured rather than read: install the wheel with --no-deps into a
throwaway venv, `import kalast`, and add whatever the ModuleNotFoundError
names, until the import returns.

    python tools/bundle_closure.py [--version 0.5.0]

Prints the packages in the order they were demanded, and the size of the
result. Writing the file is left to a human, because the pinned lower bounds
in it are a judgement rather than a measurement -- and because the closure
is only half the list. The other half is what the *shipped examples* call,
which no import of `kalast` reveals: `kalast.plot` and `kalast.tpm` defer
their submodules, so matplotlib and scipy do not appear here even though
three examples in the bundle need them.
"""

import argparse
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

# An import name that is not its package name on PyPI.
PACKAGE = {"PIL": "pillow", "dateutil": "python-dateutil"}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--version", default="", help="kalast version; default the local wheel")
    ap.add_argument("--keep", action="store_true", help="do not delete the venv")
    args = ap.parse_args()

    root = pathlib.Path(tempfile.mkdtemp(prefix="kalast-closure-"))
    venv = root / "venv"
    subprocess.run([sys.executable, "-m", "venv", str(venv)], check=True)
    py = venv / ("Scripts" if sys.platform == "win32" else "bin") / "python"

    spec = f"kalast=={args.version}" if args.version else "kalast"
    subprocess.run([py, "-m", "pip", "-q", "install", "--no-deps", spec], check=True)

    added: list[str] = []
    for _ in range(40):
        done = subprocess.run(
            [py, "-c", "import kalast"], capture_output=True, text=True
        )
        if done.returncode == 0:
            break
        found = re.search(r"No module named '([A-Za-z0-9_]+)'", done.stderr)
        if not found:
            print(done.stderr, file=sys.stderr)
            return 1
        pkg = PACKAGE.get(found.group(1), found.group(1))
        print(f"  {found.group(1)} -> pip install {pkg}", flush=True)
        subprocess.run([py, "-m", "pip", "-q", "install", pkg], check=True)
        added.append(pkg)
    else:
        print("gave up after 40 rounds", file=sys.stderr)
        return 1

    print("\nclosure:", " ".join(added))
    site = next((venv / "lib").glob("python*/site-packages"), None) or venv
    total = sum(f.stat().st_size for f in site.rglob("*") if f.is_file())
    print(f"installed size: {total / 1e6:.0f} MB")
    if not args.keep:
        shutil.rmtree(root, ignore_errors=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
