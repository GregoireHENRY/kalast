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
    app.config.open_in_background = True
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
    while True:
        alive = app.step()
        # `step()` is also False while a run is pending -- that is how a
        # driven script's loop is asked to end -- so only "nothing pending"
        # means closed.
        asked = app.take_script_request()
        if not alive and asked is None:
            break
        frames += 1
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


RESTART_SCRIPT = textwrap.dedent("""
    import sys
    import numpy
    from kalast.app import App

    app = App()
    app.simulation.load_mesh(
        path="res/plane_crater_1024-5000_h=0.437.obj", mat=numpy.eye(4)
    )
    sys._runs = getattr(sys, "_runs", 0) + 1
    while app.running:
        st = app.simulation.state
        if st.is_paused:
            st.is_paused = False  # the hold at the start: press Play
        if sys._runs == 1 and st.iteration >= 5:
            sys._restarted_at = st.iteration
            app.restart_script()  # what the Restart button does
        if sys._runs >= 2 and st.iteration >= 3:
            app.close()
        app.step()
""")

RESTART_PROBE = textwrap.dedent("""
    import sys
    from kalast import editor
    from kalast.app import App

    app = App()
    app.config.open_in_background = True
    app.config.editor = True
    app.config.width, app.config.height = 640, 480
    editor.capture_output(app)
    app.set_script("restart_probe.py", open(sys.argv[1]).read())
    app.simulation.state.is_paused = True
    app.restart_script()

    while True:
        alive = app.step()
        asked = app.take_script_request()
        if not alive and asked is None:
            break
        if asked:
            path, source, paused = asked
            app.simulation.reset()
            if paused:
                app.simulation.state.pause_at = 1
            app.simulation.state.is_paused = False
            app.script_ran = True
            editor.run_toplevel(app, source, path)
    print("PROBE %d %d %d %d" % (getattr(sys, "_runs", 0), getattr(sys, "_restarted_at", -1),
                                 len(app.simulation.bodies), app.simulation.state.iteration),
          flush=True)
""")


def test_restart_ends_a_driven_loop_and_runs_the_script_again() -> None:
    """Restart did nothing for a script that drives its own loop.

    The request was recorded inside the frame and taken between two editor
    frames -- but a `while app.running:` script never gave the editor a
    frame back, because `running` and `step()` only went False when the
    window closed. Now another pending run ends the loop the same way, the
    script returns, and the editor runs it again from the top over a cleared
    scene. The script here presses Restart itself at iteration 5, then
    closes the window on its second run.
    """
    script = ROOT / "tests" / "_restart_script.py"
    harness = ROOT / "tests" / "_restart_probe.py"
    script.write_text(RESTART_SCRIPT)
    harness.write_text(RESTART_PROBE)
    try:
        r = subprocess.run(
            [sys.executable, str(harness), str(script)],
            capture_output=True, text=True, timeout=120, cwd=ROOT,
        )
    finally:
        script.unlink(missing_ok=True)
        harness.unlink(missing_ok=True)
    line = next((l for l in r.stdout.splitlines() if l.startswith("PROBE")), None)
    assert line, "probe produced nothing:\n" + r.stdout + r.stderr
    _, runs, restarted_at, bodies, it = line.split()
    assert int(runs) == 2, f"the script should have run twice, ran {runs}"
    assert int(restarted_at) >= 5, "the first run should have got to iteration 5"
    assert int(bodies) == 1, f"the scene must be cleared between runs, has {bodies} bodies"
    assert 3 <= int(it) < 5, f"the second run should have stopped at 3, was at {it}"


def test_no_script_does_not_advance() -> None:
    it, paused, bodies, drawn = probe(None)
    assert bodies == 0, "nothing should be loaded"
    assert it == 0, f"the counter must hold at 0 over an empty scene, got {it}"
    assert paused


def test_a_script_is_built_and_held_at_iteration_zero() -> None:
    it, paused, bodies, drawn = probe("examples/crater_self_shadow/fn.py")
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
    _, paused, _, drawn = probe("examples/crater_self_shadow/fn.py", steps=3)
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
        app.config.open_in_background = True
        app.config.width, app.config.height = 320, 240
        app.config.vsync = False

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
        app.config.open_in_background = True
        app.config.vsync = False
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
        app.config.open_in_background = True
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
