#!/usr/bin/env python
"""What the editor does when it opens, for both ways of opening it.

There are two states and they are easy to confuse, which is how the second
one broke twice: with **no script** nothing should advance, because there is
no simulation worth advancing and a counter climbing over an empty scene
gives you nothing to press. With **a script** it should build the scene and
hold at iteration 0, because a black viewport waiting for Play is no way to
open a file.

The first regressed when the second was added: the auto-run covers the case
where a script was named and says nothing about the case where none was.

Opens a real window briefly -- there is no way to check this without one.
"""

import pathlib
import subprocess
import sys
import textwrap

ROOT = pathlib.Path(__file__).resolve().parent.parent

PROBE = textwrap.dedent("""
    import sys
    import numpy  # noqa
    from kalast import editor
    from kalast.app import App

    script = sys.argv[1] if len(sys.argv) > 1 else None

    app = App()
    app.config.editor = True
    app.config.width, app.config.height = 640, 480
    editor.capture_output(app)
    if script:
        app.set_script(script, open(script).read().replace("app.start()", ""))

    # Mirrors kalast/__main__.py.
    app.simulation.state.is_paused = True
    if script:
        app.restart_script()

    frames = 0
    while app.step():
        frames += 1
        asked = app.take_script_request()
        if asked:
            path, source, paused = asked
            app.simulation.reset()
            if paused:
                app.simulation.state.pause_at = 1
            app.simulation.state.is_paused = False
            app.script_ran = True
            editor.run_toplevel(app, source, path)
        if frames == 120:
            st = app.simulation.state
            print("PROBE %d %s %d %d" % (st.iteration, st.is_paused,
                                         len(app.simulation.bodies),
                                         app.drawn_iteration), flush=True)
            break
""")


def probe(script: str | None) -> tuple[int, bool, int, int]:
    path = ROOT / "tests" / "_editor_probe.py"
    path.write_text(PROBE)
    try:
        args = [sys.executable, str(path)] + ([script] if script else [])
        r = subprocess.run(args, capture_output=True, text=True, timeout=120, cwd=ROOT)
    finally:
        path.unlink(missing_ok=True)
    for line in r.stdout.splitlines():
        if line.startswith("PROBE"):
            _, it, paused, bodies, drawn = line.split()
            return int(it), paused == "True", int(bodies), int(drawn)
    raise AssertionError("probe produced nothing:\n" + r.stdout + r.stderr)


def test_no_script_does_not_advance() -> None:
    it, paused, bodies, drawn = probe(None)
    assert bodies == 0, "nothing should be loaded"
    assert it == 0, f"the counter must hold at 0 over an empty scene, got {it}"
    assert paused


def test_a_script_is_built_and_held_at_iteration_zero() -> None:
    it, paused, bodies, drawn = probe("examples/crater_self_shadow/main.py")
    assert bodies == 1, "the script's mesh should be loaded"
    assert it == 1, f"exactly one iteration should have run, got {it}"
    assert paused, "and then it should hold"
    # `state.iteration` counts iterations finished; the toolbar shows the one
    # on screen, and the screen is showing iteration 0.
    assert drawn == 0, f"the frame shown is iteration 0, got {drawn}"


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
