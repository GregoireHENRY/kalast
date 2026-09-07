"""Open the kalast editor.

    python -m kalast                       empty scene
    python -m kalast path/to/mesh.obj      with meshes loaded
    python -m kalast examples/.../step.py  with a script in the editor

The editor is a second entry point, not a change to how scripts run: a script
that calls `app.start()` or drives `app.step()` still opens the plain render
window from a terminal, exactly as before.
"""

import sys
from pathlib import Path

import numpy

from kalast import editor
from kalast.app import App


def main(argv: list[str] | None = None) -> int:
    argv = sys.argv[1:] if argv is None else argv

    app = App()
    app.simulation.config.title = "kalast"
    # Physical pixels, so this is 1280x800 points on a 2x display. The editor
    # needs the room: three panels plus a viewport at 800 points wide leaves
    # the viewport a sliver.
    app.config.width = 2560
    app.config.height = 1600

    editor.capture_output(app)
    app.script_runner = editor.make_runner()

    for arg in argv:
        path = Path(arg)
        if path.suffix == ".py":
            # Loaded into the buffer, not run: opening a file should show it,
            # and Run is one click away.
            app.set_script(str(path), path.read_text())
        else:
            app.simulation.load_mesh(path=str(path), mat=numpy.eye(4), flatten=True)

    app.start_editor()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
