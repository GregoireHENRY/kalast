"""Open the kalast editor.

    python -m kalast                       empty scene
    python -m kalast path/to/mesh.obj      with meshes loaded
    python -m kalast examples/.../main.py  with that script loaded, ready to
                                           play

The editor is a **mode**, not a second way to run. A script keeps its own
shape: one that calls `app.start()` owns the loop, one that drives
`while app.step():` drives it, and either way the frame also draws the panels.
Running the same file from a terminal gives the plain window, as before.

The loop lives here rather than in `app.start_editor()`, because a script has
to run *between* frames -- a script with its own `while app.step():` cannot
run inside the frame that is drawing it.
"""

import sys
from pathlib import Path

import numpy

from kalast import editor
from kalast.app import App


def main(argv: list[str] | None = None) -> int:
    argv = sys.argv[1:] if argv is None else argv

    app = App()
    app.config.editor = True
    # Physical pixels, so 1280x800 points on a 2x display. Three panels and a
    # viewport need the room.
    app.config.width = 2560
    app.config.height = 1600
    app.simulation.config.title = "kalast"

    editor.capture_output(app)

    for arg in argv:
        if arg.startswith("-"):
            continue
        path = Path(arg)
        if path.suffix == ".py":
            app.set_script(str(path), path.read_text())
        else:
            app.simulation.load_mesh(path=str(path), mat=numpy.eye(4), flatten=True)

    # Loaded, not played. An editor that starts running before you press Play
    # gives you nothing to press and no way back to the start.
    app.simulation.state.is_paused = True

    while app.step():
        asked = app.take_script_request()
        if asked is None:
            continue
        path, source, paused = asked
        # `load_mesh` appends, so a run without this stacks the scene: two
        # craters, and Restart looking like it did nothing.
        app.simulation.reset()
        # Play rebuilds *and* runs; Restart rebuilds and stops at the start.
        #
        # A driven script left paused still goes round its own loop -- there
        # is no holding a `while` the script owns -- but nothing advances, so
        # the scene sits still and the window stays responsive. Play releases
        # it. What Play must not do is leave it paused: a driven script's exit
        # test is usually on `state.iteration`, so it would never finish, and
        # pressing Play and seeing nothing move is the confusion this button
        # started as.
        app.simulation.state.is_paused = paused
        # Between frames, so a script that drives its own loop nests here
        # rather than inside the frame -- and runs to completion before this
        # loop resumes.
        app.script_ran = True
        editor.run_toplevel(app, source, path)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
