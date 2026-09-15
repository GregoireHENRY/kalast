#!/usr/bin/env python
"""Every config option must be reachable from Python.

The third guard of the same shape as `test_stubs.py` and
`test_config_panel.py`, and the one the other two cannot stand in for. A
field added to `Config` in Rust gets a widget in the editor's panel and a
line in the `.pyi` automatically, because both are *generated* -- but the
`#[getter]`/`#[setter]` pair was written by hand -- it is generated now, by
`tools/gen_bindings.py`, but a field marked `:py_custom:` still relies on
someone writing it. Forget it and the option is complete everywhere except
from a script:

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
EXEMPT: dict[str, set[str]] = {}


def fields(struct: str) -> list[str]:
    src = CONFIG.read_text()
    m = re.search(r"pub struct %s \{(.*?)\n\}" % struct, src, re.S)
    assert m, f"{struct} not found in {CONFIG}"
    return re.findall(r"^\s*pub (\w+): ", m.group(1), re.M)


def groups() -> list[tuple[str, str]]:
    """`(field, RustType)` for every sub-struct of `Config`, in order."""
    src = CONFIG.read_text()
    m = re.search(r"pub struct Config \{(.*?)\n\}", src, re.S)
    assert m
    return re.findall(r"^\s*pub (\w+): ([A-Za-z_]+),", m.group(1), re.M)


def targets(app):
    """The structs to check, each with the Python object that carries them.

    `Config` is a struct of sub-structs, and the Python surface follows it:
    `app.simulation.config.<group>` is a live view onto one sub-struct, so
    each group is checked on its own view. `AppConfig` is flat.
    """
    out = [(gtype, getattr(app.simulation.config, g)) for g, gtype in groups()]
    out.append(("AppConfig", app.config))
    return out


def bound_as(struct: str, name: str) -> str:
    """Python name for a Rust field. The same, now that nothing is flattened."""
    return name


def test_every_config_field_is_bound() -> None:
    """`app.config.<field>` exists for every field of the Rust struct."""
    app = App()
    app.config.open_in_background = True
    missing = []
    for struct, obj in targets(app):
        have = set(dir(obj))
        for name in fields(struct):
            if bound_as(struct, name) in have or name in EXEMPT.get(struct, ()):
                continue
            missing.append(f"{struct}.{name}")
    assert not missing, "no Python getter for: " + ", ".join(missing)


def test_every_bound_field_is_writable() -> None:
    """A getter without a setter is a read-only option, which none are.

    Reading works, so the field looks bound; the failure only appears when a
    script tries to set it, which is the only thing anyone does with it.
    """
    app = App()
    app.config.open_in_background = True
    problems = []
    for struct, obj in targets(app):
        for name in fields(struct):
            name = bound_as(struct, name)
            if name in EXEMPT.get(struct, ()) or not hasattr(obj, name):
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
