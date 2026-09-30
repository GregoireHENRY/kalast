#!/usr/bin/env python
"""`res/examples-shipped.txt` holds every example file kalast has shipped: each
release tag's and the working tree's, as `tools/gen_examples_shipped.py`
writes them. Missing one, an update would take that file, exactly as kalast
shipped it, for one the user changed, and keep it in their `scripts/`.

Regenerate after changing an example:

    python tools/gen_examples_shipped.py

Skipped outside a git checkout, where there are no tags to read.
"""

import importlib.util
import pathlib
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent


def generator():
    spec = importlib.util.spec_from_file_location("gen_examples_shipped", ROOT / "tools" / "gen_examples_shipped.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def in_a_checkout() -> bool:
    return shutil.which("git") is not None and subprocess.run(
        ["git", "rev-parse", "--git-dir"], cwd=ROOT, capture_output=True
    ).returncode == 0


def test_every_shipped_example_is_listed():
    gen = generator()
    missing = (gen.tagged() | gen.working_tree()) - gen.read()
    assert not missing, (
        f"res/examples-shipped.txt is missing {len(missing)} file(s), "
        f"{', '.join(sorted(name for _, name in missing)[:5])}: run tools/gen_examples_shipped.py"
    )


def test_the_fingerprint_is_the_one_kalast_computes():
    # The values `example_fingerprint`'s test in src/app/update.rs checks:
    # FNV-1a's published ones, a line ending either way the same, and a
    # carriage return on its own the file's.
    gen = generator()
    assert gen.fingerprint(b"") == "cbf29ce484222325"
    assert gen.fingerprint(b"a") == "af63dc4c8601ec8c"
    assert gen.fingerprint(b"foobar") == "85944171f73967e8"
    assert gen.fingerprint(b"x = 1\r\ny = 2\r\n") == gen.fingerprint(b"x = 1\ny = 2\n") == "5e4216b9c8c23ae7"
    assert gen.fingerprint(b"\r\r\n") == "083cb407b4f40f36"


if __name__ == "__main__":
    if not in_a_checkout():
        print("skip: not a git checkout")
        sys.exit(0)
    test_every_shipped_example_is_listed()
    test_the_fingerprint_is_the_one_kalast_computes()
    print("ok")
