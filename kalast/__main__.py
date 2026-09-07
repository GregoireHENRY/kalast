"""Open the kalast editor.

    python -m kalast                     empty scene
    python -m kalast path/to/mesh.obj    with a mesh loaded

The editor is a second entry point, not a change to how scripts run: a script
that calls `app.start()` or drives `app.step()` still opens the plain render
window, exactly as before.
"""

import sys

import numpy

from kalast.app import App


def main(argv: list[str] | None = None) -> int:
    argv = sys.argv[1:] if argv is None else argv

    app = App()
    app.config.title = "kalast"
    app.config.width = 1600
    app.config.height = 1000
    for path in argv:
        app.simulation.load_mesh(path=path, mat=numpy.eye(4), flatten=True)
    app.start_editor()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
