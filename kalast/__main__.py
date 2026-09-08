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
        # `.rs` opens in the panel like a script, but it is a Rust example --
        # a separate program that links kalast -- so it is built and launched
        # from there rather than run in this process.
        if path.suffix in (".py", ".rs"):
            app.set_script(str(path), path.read_text())
        elif path.suffix == ".obj":
            app.simulation.load_mesh(path=str(path), mat=numpy.eye(4), flatten=True)
        else:
            # Anything else used to be handed to the OBJ parser, which read a
            # Rust file as a mesh, produced no vertices, and took the process
            # down with "buffer slice can not be empty".
            print(f"don't know what to do with {path}: expected .py, .rs or .obj")

    # Held, always. Nothing here owns a simulation worth advancing until a
    # script has built one, and an editor whose counter climbs over an empty
    # scene gives you nothing to press and no way back to the start.
    #
    # This was here, went out with the auto-run below, and had to come back:
    # the auto-run covers the case where a script *was* named, and says
    # nothing about the case where none was.
    app.simulation.state.is_paused = True

    # A script named on the command line is *shown*: built and rendered at
    # iteration 0, then held. Opening one and getting a black viewport until
    # you find Play is no way to open a file. The run below clears the pause
    # for exactly one iteration and `pause_at` puts it back.
    # Only Python auto-runs: a Rust example has to be compiled first, and
    # there is nothing to show until it is.
    if any(not a.startswith("-") and a.endswith(".py") for a in argv):
        app.restart_script()

    while app.step():
        asked = app.take_script_request()
        if asked is None:
            continue
        path, source, paused = asked
        # `load_mesh` appends, so a run without this stacks the scene: two
        # craters, and Restart looking like it did nothing.
        app.simulation.reset()
        # Play rebuilds and runs. Restart rebuilds and runs *one iteration*,
        # then stops -- not "does not run at all", which would leave a black
        # viewport and nothing to look at. One iteration means the callbacks
        # fire once, so a script that places its bodies per iteration shows
        # them where iteration 0 puts them rather than at the origin.
        #
        # A driven script left stopped still goes round its own loop -- there
        # is no holding a `while` the script owns -- but nothing advances, so
        # the scene sits still and the window stays responsive.
        if paused:
            app.simulation.state.pause_at = 1
        app.simulation.state.is_paused = False
        # Between frames, so a script that drives its own loop nests here
        # rather than inside the frame -- and runs to completion before this
        # loop resumes.
        app.script_ran = True
        editor.run_toplevel(app, source, path)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
