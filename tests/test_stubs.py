"""The .pyi stubs must match the Rust source and the built module.

Stubs exist so an editor can complete `app.config.<tab>`; a compiled
extension tells a static analyser nothing. They are generated rather than
hand-written because hand-written ones rot -- this repo already carried a set
of .pyi files that had been commented out entirely and so completed nothing,
which is worse than having none, since it looks like the surface is covered.

Two failure modes, one test each.
"""

import ast
import pathlib
import subprocess
import sys

import kalast

ROOT = pathlib.Path(__file__).resolve().parent.parent


def test_stubs_are_regenerated():
    """Fails when a #[pyclass] changed and the stubs were not regenerated."""
    r = subprocess.run(
        [sys.executable, str(ROOT / "tools" / "gen_stubs.py"), "--check"],
        capture_output=True,
        text=True,
    )
    assert r.returncode == 0, r.stderr or r.stdout


def test_every_stub_is_valid_python():
    """A stub that does not parse is worse than none.

    A type checker discards the whole file on a syntax error and silently
    offers no completion, which looks identical to having no stub at all.
    This caught Rust `//` comments leaking into a generated argument list.
    """
    for pyi in sorted((ROOT / "kalast").rglob("*.pyi")):
        try:
            # compile(), not ast.parse(): parsing accepts duplicate argument
            # names, which is exactly what a bug in the generator produced --
            # `def load_mesh(self, object: str, object: list)`.
            compile(pyi.read_text(), str(pyi), "exec")
        except SyntaxError as e:
            raise AssertionError(f"{pyi.relative_to(ROOT)}: {e}") from e


def test_every_annotation_resolves():
    """Names in annotations must be defined or imported in that file.

    An unresolved name makes a checker treat the attribute as an error, so
    `app.config` would complete nothing even though `Config` exists.
    """
    builtins_ = {"int", "float", "str", "bool", "object", "None", "list",
                 "tuple", "dict", "numpy", "Any"}
    problems = []
    for pyi in sorted((ROOT / "kalast").rglob("*.pyi")):
        tree = ast.parse(pyi.read_text())
        known = {n.name for n in tree.body if isinstance(n, ast.ClassDef)}
        known |= {a.name for n in tree.body if isinstance(n, ast.ImportFrom)
                  for a in n.names}
        known |= {n.names[0].name for n in tree.body if isinstance(n, ast.Import)}
        for cls in (n for n in tree.body if isinstance(n, ast.ClassDef)):
            for b in cls.body:
                if not isinstance(b, ast.AnnAssign):
                    continue
                for t in ast.walk(b.annotation):
                    if isinstance(t, ast.Name) and t.id not in known | builtins_:
                        problems.append(
                            f"{pyi.relative_to(ROOT)}: {cls.name}.{b.target.id}"
                            f" -> unresolved {t.id}"
                        )
    assert not problems, "\n".join(problems)


def _stub_members(pyi: str, cls: str) -> set[str]:
    tree = ast.parse((ROOT / pyi).read_text())
    for n in tree.body:
        if isinstance(n, ast.ClassDef) and n.name == cls:
            return {b.target.id for b in n.body if isinstance(b, ast.AnnAssign)} | {
                b.name
                for b in n.body
                if isinstance(b, ast.FunctionDef) and not b.name.startswith("__")
            }
    raise AssertionError(f"{cls} not found in {pyi}")


def test_stubs_match_the_built_module():
    """Fails when a stub promises something the module does not have.

    The generator reads the Rust source, so it can drift from what pyo3
    actually exports -- a `#[new]` it failed to recognise, say, which would
    offer a `new()` that does not exist. Comparing against `dir()` on real
    objects catches that.
    """
    app = kalast.app.App()
    cases = [
        # App first: `app.config.<tab>` is the whole point, and it was the one
        # class this test did not cover when the stubs first shipped -- so a
        # `get_simulation` that pyo3 exposes as `simulation`, and three
        # setter-only attributes that were missing outright, both slipped past.
        ("kalast/app/_core.pyi", "App", app),
        ("kalast/app/config.pyi", "Config", app.config),
        ("kalast/app/config.pyi", "Hud", kalast.app.Hud("x")),
        ("kalast/app/simulation.pyi", "Simulation", app.simulation),
        ("kalast/app/simulation.pyi", "State", app.simulation.state),
        ("kalast/app/frame.pyi", "Eye", app.simulation.camera),
        ("kalast/app/frame.pyi", "Projection", app.simulation.camera.projection),
        ("kalast/entity.pyi", "Body", kalast.entity.DEIMOS),
        ("kalast/entity.pyi", "Camera", kalast.entity.TIRI),
        ("kalast/tpm/properties.pyi", "Properties", kalast.tpm.properties.DEIMOS),
    ]
    problems = []
    for pyi, cls, obj in cases:
        real = {m for m in dir(obj) if not m.startswith("_")}
        stub = _stub_members(pyi, cls)
        if missing := real - stub:
            problems.append(f"{cls}: stub is missing {sorted(missing)}")
        if extra := stub - real:
            problems.append(f"{cls}: stub invents {sorted(extra)}")
    assert not problems, "\n".join(problems)


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
