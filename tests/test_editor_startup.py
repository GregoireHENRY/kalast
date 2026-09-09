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
    steps = int(sys.argv[2]) if len(sys.argv) > 2 else 0

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
            for _ in range(steps):
                # Exactly what the Step button does.
                st = app.simulation.state
                st.pause_at = st.iteration + 1
                st.is_paused = False
                for _ in range(15):
                    app.step()
            st = app.simulation.state
            print("PROBE %d %s %d %d" % (st.iteration, st.is_paused,
                                         len(app.simulation.bodies),
                                         app.drawn_iteration), flush=True)
            break
""")


def probe(script: str | None, steps: int = 0) -> tuple[int, bool, int, int]:
    path = ROOT / "tests" / "_editor_probe.py"
    path.write_text(PROBE)
    try:
        args = [sys.executable, str(path)]
        if script:
            args += [script, str(steps)]
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


def test_step_advances_exactly_one_rendered_iteration() -> None:
    """Step used to move the counter without rendering anything.

    The UI is drawn near the end of a frame, so Step unpaused after that
    frame had already skipped its callbacks -- and `update()`, re-reading the
    flag, counted an iteration that never ran. `pause_at` fired straight
    afterwards, so the counter moved, nothing was drawn, and it looked stuck.
    """
    _, paused, _, drawn = probe("examples/crater_self_shadow/main.py", steps=3)
    assert drawn == 3, f"three Steps should show iteration 3, got {drawn}"
    assert paused, "and it should be held again afterwards"


def test_one_step_is_one_frame() -> None:
    """`step()` must draw exactly one frame, not "at least one".

    The redraw handler re-requests a redraw on entry, so a single
    `pump_app_events` dispatches every redraw it can feed itself. A tight Rust
    loop got about five frames per `step()` -- five `update()`s, with the work
    done before the call applied to only the first of them, which is exactly
    the mismatch between a moved Sun and the shadow map that the
    before/after-`step()` idiom exists to avoid. Python's slower loop happened
    to get one, so it never showed here.
    """
    path = ROOT / "tests" / "_step_probe.py"
    path.write_text(textwrap.dedent("""
        from kalast.app import App

        app = App()
        app.config.width, app.config.height = 320, 240
        app.simulation.config.vsync = False

        steps = 0
        while steps < 40 and app.step():
            steps += 1
        print("STEPS %d %d" % (steps, app.simulation.state.iteration), flush=True)
    """))
    try:
        r = subprocess.run([sys.executable, str(path)], capture_output=True,
                           text=True, timeout=120, cwd=ROOT)
    finally:
        path.unlink(missing_ok=True)
    line = next((l for l in r.stdout.splitlines() if l.startswith("STEPS")), None)
    assert line, "probe produced nothing:\n" + r.stdout + r.stderr
    _, steps, iteration = line.split()
    assert steps == "40", f"the loop should have run 40 times, got {steps}"
    assert iteration == steps, (
        f"{steps} steps must be {steps} iterations, got {iteration}"
    )


def test_a_driven_script_keeps_its_own_loop_in_the_editor() -> None:
    """`while app.step():` has to work inside the editor, not just in a terminal.

    It briefly did not. The engine's editor loop was entered through
    `inner.borrow_mut()` held for the whole session, so a script calling back
    into the app met an already-borrowed `RefCell` -- and the proxy raised
    instead, blaming the frame. The frame was never the obstacle: a script
    runs *between* frames. The front door turns `editor_tick` one step at a
    time now, holding nothing while the script runs.
    """
    script = ROOT / "tests" / "_driven_script.py"
    probe = ROOT / "tests" / "_driven_probe.py"
    script.write_text(textwrap.dedent("""
        from kalast.app import App

        app = App()
        app.simulation.config.vsync = False
        # A script named on the command line is shown and then *held* --
        # `pause_at = 1`. A driven script that wants to run says so.
        app.simulation.state.pause_at = None
        app.simulation.state.is_paused = False

        n = 0
        while app.running and n < 25:
            app.step()
            n += 1
        print("DRIVEN %d %d" % (n, app.simulation.state.iteration), flush=True)
    """))
    probe.write_text(textwrap.dedent(f"""
        from kalast import editor
        from kalast.app import App

        app = App()
        app.config.width, app.config.height = 480, 360
        editor.capture_output(app)

        def runner(a, source, path):
            editor.run_toplevel(a, source, path)
            # The real app, not the script's proxy: ends the editor loop so
            # the test terminates.
            a.close()

        app.run_editor([r"{script}"], runner)
    """))
    try:
        r = subprocess.run([sys.executable, str(probe)], capture_output=True,
                           text=True, timeout=180, cwd=ROOT)
    finally:
        probe.unlink(missing_ok=True)
        script.unlink(missing_ok=True)

    assert "does not work inside the editor" not in r.stdout + r.stderr, (
        "the driven loop was refused:\n" + r.stdout + r.stderr
    )
    line = next((l for l in r.stdout.splitlines() if l.startswith("DRIVEN")), None)
    assert line, "the script never finished its loop:\n" + r.stdout + r.stderr
    _, steps, iteration = line.split()
    assert steps == "25", f"the script should have stepped 25 times, got {steps}"
    assert int(iteration) >= 25, (
        f"its steps should have advanced the simulation, got {iteration}"
    )


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
