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

    `Config` is a struct of sub-structs, one per group, so this walks each
    group's own fields and expects `c.<group>.<field>`; `AppConfig` is flat and
    expects `a.<field>`. A field may opt out with `/// :skip:`, which is
    deliberate and visible in the Rust source; anything else must appear.
    """
    src = CONFIG.read_text()
    panel = PANEL.read_text()

    def skipped(name: str) -> bool:
        # The whole doc block, not just the line above `pub`: `:skip:` is
        # allowed anywhere in it, and the generator reads it that way.
        block = re.search(rf"((?:^[ \t]*///[^\n]*\n)*)[ \t]*pub {name}:", src, re.M)
        return bool(block and ":skip:" in block.group(1))

    def types(struct: str) -> list[tuple[str, str]]:
        m = re.search(r"pub struct %s \{(.*?)\n\}" % struct, src, re.S)
        assert m, f"{struct} not found in {CONFIG}"
        return re.findall(r"^\s*pub (\w+): ([^,\n]+),", m.group(1), re.M)

    missing = []
    for group, gtype in types("Config"):
        members = fields(gtype)
        assert members, f"Config.{group}: {gtype} has no fields -- not a group?"
        for name in members:
            if not skipped(name) and f"c.{group}.{name}" not in panel:
                missing.append(f"c.{group}.{name}")
    for name in fields("AppConfig"):
        if not skipped(name) and f"a.{name}" not in panel:
            missing.append(f"a.{name}")
    assert not missing, "no widget for: " + ", ".join(missing)


if __name__ == "__main__":
    # Runnable without pytest, which is not installed here.
    failures = 0
    for name, fn in sorted(globals().items()):
        if name.startswith("test_") and callable(fn):
            try:
                fn()
                print(f"ok   {name}")
            except Exception as e:
                failures += 1
                print(f"FAIL {name}\n     {type(e).__name__}: {e}")
    raise SystemExit(failures)
