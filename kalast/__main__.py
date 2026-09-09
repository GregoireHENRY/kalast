"""Open the kalast editor.

    python -m kalast                       empty scene
    python -m kalast path/to/mesh.obj      with meshes loaded
    python -m kalast examples/.../main.py  with that script loaded, ready to
                                           play

The editor is a **mode**, not a second way to run. A script keeps its own
shape: one that calls `app.start()` owns the loop, one that drives
`while app.step():` drives it, and either way the frame also draws the panels.
Running the same file from a terminal gives the plain window, as before.

**This is a wrapper.** The loop is `App::run_editor` in the engine, and
`cargo run --bin kalast` runs the same one. It used to live here, which meant
the editor could not be opened from Rust at all -- see "Rust core, Python
wrapper" in `CLAUDE.md`.

What is left here is the part that genuinely needs an interpreter: executing a
`.py` script. Rust owns the loop and calls back for that, rather than Python
owning a loop Rust cannot enter. A script has to run *between* frames -- one
with its own `while app.step():` cannot run inside the frame that is drawing
it -- and the engine's loop calls back at exactly that point.
"""

import sys

from kalast import editor
from kalast.app import App


def main(argv: list[str] | None = None) -> int:
    argv = sys.argv[1:] if argv is None else argv

    app = App()
    editor.capture_output(app)

    # Everything else -- argv parsing, the pause and auto-run policy, the
    # rebuild-on-Play cycle -- is in the engine. The callback is the only
    # thing Rust cannot do without CPython.
    app.run_editor(argv, editor.make_runner())

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
