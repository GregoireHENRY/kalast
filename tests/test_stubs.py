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
