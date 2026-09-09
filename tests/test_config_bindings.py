#!/usr/bin/env python
"""Every config option must be reachable from Python.

The third guard of the same shape as `test_stubs.py` and
`test_config_panel.py`, and the one the other two cannot stand in for. A
field added to `Config` in Rust gets a widget in the editor's panel and a
line in the `.pyi` automatically, because both are *generated* -- but the
`#[getter]`/`#[setter]` pair in `src/py/app/config.rs` is written by hand.
Forget it and the option is complete everywhere except from a script:

    AttributeError: 'builtins.Config' object has no attribute 'facet_labels'

The stubs cannot catch it, since they are generated from the wrapper and so
agree with it about the field not existing. This has bitten twice --
`debug_light_cube_fit`, then `facet_labels`.
"""

import pathlib
import re

from kalast.app import App

ROOT = pathlib.Path(__file__).resolve().parent.parent
CONFIG = ROOT / "src/app/config.rs"

# Fields that are deliberately not Python-visible, with why. Kept short: an
# entry here is a decision, not a way to silence the test.
EXEMPT = {
    "Config": set(),
    "Colorbar": set(),
    "AppConfig": set(),
}


def fields(struct: str) -> list[str]:
    src = CONFIG.read_text()
    m = re.search(r"pub struct %s \{(.*?)\n\}" % struct, src, re.S)
    assert m, f"{struct} not found in {CONFIG}"
    return re.findall(r"^\s*pub (\w+): ", m.group(1), re.M)


def targets(app):
    """The structs to check, each with the Python object that carries them.

    `Colorbar` has no object of its own: its fields are flattened onto
    `Config` as `colorbar_*`, which is a nicer surface than a sub-object and
    is why the mapping below exists.
    """
    return [
        ("Config", app.simulation.config),
        ("Colorbar", app.simulation.config),
        ("AppConfig", app.config),
    ]


def bound_as(struct: str, name: str) -> str:
    """Python name for a Rust field, where the two differ."""
    if struct != "Colorbar":
        return name
    return "colorbar" if name == "enabled" else f"colorbar_{name}"


def test_every_config_field_is_bound() -> None:
    """`app.config.<field>` exists for every field of the Rust struct."""
    app = App()
    missing = []
    for struct, obj in targets(app):
        have = set(dir(obj))
        for name in fields(struct):
            if bound_as(struct, name) in have or name in EXEMPT[struct]:
                continue
            missing.append(f"{struct}.{name}")
    assert not missing, "no Python getter for: " + ", ".join(missing)


def test_every_bound_field_is_writable() -> None:
    """A getter without a setter is a read-only option, which none are.

    Reading works, so the field looks bound; the failure only appears when a
    script tries to set it, which is the only thing anyone does with it.
    """
    app = App()
    problems = []
    for struct, obj in targets(app):
        for name in fields(struct):
            name = bound_as(struct, name)
            if name in EXEMPT[struct] or not hasattr(obj, name):
                continue
            try:
                setattr(obj, name, getattr(obj, name))
            except AttributeError as e:
                problems.append(f"{struct}.{name}: {e}")
            except Exception:
                # Any other error means a setter ran and rejected the value,
                # which is all this test asks. `colormap` refuses to take
                # back its own empty default, for instance.
                pass
    assert not problems, "read-only: " + "; ".join(problems)


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
