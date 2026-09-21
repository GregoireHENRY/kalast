"""`import kalast` must not drag in the plotting and solver stack.

`kalast/__init__.py` imports every subpackage, and `kalast/plot` and
`kalast/tpm` used to import all of *their* submodules -- so a script that
only renders a mesh loaded matplotlib, scipy and pyarrow. In a release
bundle that is 174 of the 283 MB of site-packages, and pyarrow alone, which
nothing shipped uses, is 120.

Both packages defer through PEP 562 `__getattr__` now. This pins the two
halves of that: the heavy things stay unimported, and every spelling that
worked before still works.

Each check runs in a fresh interpreter, because `sys.modules` is process
wide and one test importing matplotlib would hide the regression from the
next.
"""

import subprocess
import sys

HEAVY = ("scipy", "matplotlib", "pyarrow", "PIL")


def _run(code: str) -> str:
    done = subprocess.run(
        [sys.executable, "-c", code], capture_output=True, text=True
    )
    assert done.returncode == 0, f"{code}\n{done.stderr}"
    return done.stdout.strip()


def test_importing_kalast_stays_light():
    loaded = _run(
        "import sys, kalast;"
        f"print(','.join(m for m in {HEAVY!r} if m in sys.modules))"
    )
    assert loaded == "", f"import kalast pulled in {loaded}"


def test_numpy_is_still_eager():
    """Not an accident to be tidied away: `kalast.entity` and `kalast.util`
    use numpy at module level, and every example imports it anyway."""
    loaded = _run("import sys, kalast; print('numpy' in sys.modules)")
    assert loaded == "True", loaded


def test_the_old_spellings_still_work():
    for attr, want in [
        ("kalast.plot.cbar.Params.__name__", "Params"),
        ("kalast.plot.style.__name__", "kalast.plot.style"),
        ("kalast.tpm.properties.__name__", "kalast.tpm.properties"),
        ("kalast.tpm.implicit.__name__", "kalast.tpm.implicit"),
    ]:
        got = _run(f"import kalast; print({attr})")
        assert got == want, f"{attr} -> {got!r}, wanted {want!r}"


def test_reaching_for_one_imports_only_what_it_needs():
    """`plot.cbar` wants matplotlib; it must not also bring scipy and
    pyarrow, which only `plot.tool` and `plot.smap` use."""
    loaded = _run(
        "import sys, kalast; kalast.plot.cbar;"
        f"print(','.join(m for m in {HEAVY!r} if m in sys.modules))"
    )
    assert "matplotlib" in loaded, loaded
    assert "scipy" not in loaded and "pyarrow" not in loaded, loaded


def test_a_missing_name_is_an_attribute_error():
    """Not an ImportError, and not a silent `None`: `from kalast.plot import
    nope` and a typo at the prompt should both say what they mean."""
    got = _run(
        "import kalast\n"
        "try:\n"
        "    kalast.plot.nope\n"
        "except AttributeError as e:\n"
        "    print('AttributeError')\n"
    )
    assert got == "AttributeError", got


def test_dir_still_advertises_the_submodules():
    for pkg, name in [("plot", "smap"), ("tpm", "heating")]:
        got = _run(f"import kalast; print({name!r} in dir(kalast.{pkg}))")
        assert got == "True", f"dir(kalast.{pkg}) hides {name}"


if __name__ == "__main__":
    # Runnable without pytest, which is not installed here.
    failures = 0
    for name, fn in sorted(globals().items()):
        if name.startswith("test_") and callable(fn):
            try:
                fn()
                print(f"ok   {name}")
            except AssertionError as e:
                failures += 1
                print(f"FAIL {name}\n     {e}")
    raise SystemExit(failures)
