#!/usr/bin/env python
"""The editor's config panel must cover the config and stay current.

Same guard as `test_stubs.py`, for the same reason: a hand-maintained mirror
of the Rust config goes stale silently, and a missing widget is invisible --
the panel still looks complete.
"""

import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
CONFIG = ROOT / "src/app/config.rs"
PANEL = ROOT / "src/app/gui/config_panel.rs"


def fields(struct: str) -> list[str]:
    src = CONFIG.read_text()
    m = re.search(r"pub struct %s \{(.*?)\n\}" % struct, src, re.S)
    assert m, f"{struct} not found in {CONFIG}"
    return re.findall(r"^\s*pub (\w+): ", m.group(1), re.M)


def test_panel_is_regenerated() -> None:
    """The checked-in panel matches what the generator produces."""
    r = subprocess.run(
        [sys.executable, str(ROOT / "tools/gen_config_panel.py"), "--check"],
        capture_output=True,
        text=True,
    )
    assert r.returncode == 0, r.stdout + r.stderr


def test_every_field_has_a_widget() -> None:
    """No config option is silently missing from the panel.

    A field may opt out with `/// :skip:`, which is deliberate and visible in
    the Rust source; anything else must appear.
    """
    src = CONFIG.read_text()
    panel = PANEL.read_text()
    missing = []
    for struct, prefix in (("Config", "c."), ("Colorbar", "c.colorbar.")):
        for name in fields(struct):
            skipped = re.search(
                rf"///\s*:skip:\s*\n\s*pub {name}:", src
            )
            if skipped:
                continue
            if f"{prefix}{name}" not in panel:
                missing.append(f"{prefix}{name}")
    assert not missing, "no widget for: " + ", ".join(missing)


def main() -> int:
    failed = 0
    for name, fn in sorted(globals().items()):
        if not name.startswith("test_"):
            continue
        try:
            fn()
            print(f"ok   {name}")
        except AssertionError as e:
            print(f"FAIL {name}: {e}")
            failed += 1
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
