"""Open the kalast editor.

    python -m kalast                       empty scene, editor drives the loop
    python -m kalast path/to/mesh.obj      with meshes loaded
    python -m kalast examples/.../main.py  run that script, with the editor
                                           drawn around it

The editor is a **mode**, not a second way to run. A script keeps its own
shape: one that calls `app.start()` still owns the loop, one that drives
`while app.step():` still drives it, and either way the frame also draws the
panels. Running the same file from a terminal gives the plain window, exactly
as before.
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
    app.script_runner = editor.make_runner()

    script: Path | None = None
    for arg in argv:
        if arg.startswith("-"):
            # `--run` used to be needed, when a script was loaded into the
            # buffer and waited for a click. A script is executed now, so
            # there is nothing to ask for; accepted and ignored rather than
            # taken for a filename, which is what `load_mesh("--run")` did.
            continue
        path = Path(arg)
        if path.suffix == ".py":
            script = path
        else:
            app.simulation.load_mesh(path=str(path), mat=numpy.eye(4), flatten=True)

    if script is not None:
        # Shown in the panel, and executed as the program. Its own `start()`
        # or `step()` loop runs for real -- the editor draws around it.
        app.set_script(str(script), script.read_text())
        editor.run_toplevel(app, script.read_text(), str(script))

    # Whatever the script did not do. A script that owned the loop has already
    # finished by now and left `running` false; one that only set the scene up
    # -- or no script at all -- leaves the loop to us.
    if app.running:
        app.start_editor()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
