"""Running a script from inside the editor.

A kalast example is a whole program: it builds an `App`, configures it, loads
meshes, and calls `start()`. Run one unchanged inside the editor and it would
try to construct a second app and open a second window -- and a platform event
loop cannot exist twice in one process, so the second would fail outright.

So a script is executed against the app already on screen. `App()` hands back
that app, and `start()`/`start_editor()`/`close()` do nothing, because the
editor owns the loop and a script asking to own it is asking for what it is
already inside. Everything else -- config, meshes, camera, callbacks -- works
as written. That is the point: one file runs from a terminal and from here.
"""

import io
import sys
import traceback
import types
from typing import Any, Callable

import kalast
import kalast.app


class _EditorApp:
    """The live app, with the loop-owning calls neutralised.

    A proxy rather than a patch on the class: monkey-patching `App.start`
    would silently disarm it for the whole process, including for anything
    the script imports.
    """

    __slots__ = ("_app",)

    def __init__(self, app: Any) -> None:
        object.__setattr__(self, "_app", app)

    def __getattr__(self, name: str) -> Any:
        return getattr(object.__getattribute__(self, "_app"), name)

    def __setattr__(self, name: str, value: Any) -> None:
        setattr(object.__getattribute__(self, "_app"), name, value)

    def start(self) -> None:
        pass

    def start_editor(self) -> None:
        pass

    def close(self) -> None:
        pass

    def step(self) -> bool:
        # A driven loop cannot run here: this call happens *inside* the
        # editor's frame, so stepping would re-enter the frame that is
        # already running. Report the app as stopped and the script's
        # `while app.step():` exits after one pass, having done its setup.
        return False


class _Tee(io.TextIOBase):
    """Send writes to the editor log and to the real stream.

    Both, not either: the panel is what you watch while working, the terminal
    is what survives the window closing.
    """

    def __init__(self, app: Any, real: Any) -> None:
        self._app = app
        self._real = real
        self._buf = ""

    def write(self, text: str) -> int:
        self._real.write(text)
        self._buf += text
        while "\n" in self._buf:
            line, self._buf = self._buf.split("\n", 1)
            self._app.log(line)
        return len(text)

    def flush(self) -> None:
        self._real.flush()


def capture_output(app: Any) -> None:
    """Mirror `print` and tracebacks into the editor's log panel.

    Left installed for the life of the process, so a callback's output keeps
    arriving every frame after the script that set it has finished.
    """
    if not isinstance(sys.stdout, _Tee):
        sys.stdout = _Tee(app, sys.stdout)
    if not isinstance(sys.stderr, _Tee):
        sys.stderr = _Tee(app, sys.stderr)


def make_runner(app: Any) -> Callable[[str, str], None]:
    """Build the callable for `app.script_runner`."""
    proxy = _EditorApp(app)

    def _live_app(*_args: Any, **_kwargs: Any) -> _EditorApp:
        return proxy

    def run(source: str, path: str) -> None:
        # Its own module, so the script gets a clean namespace that does not
        # leak into the next run, and `__name__ == "__main__"` holds -- which
        # is what an example guards on, and unmodified examples are the point.
        module = types.ModuleType("__kalast_script__")
        module.__file__ = path or "<editor>"
        module.__name__ = "__main__"
        module.__dict__["kalast"] = kalast

        # Patched on the real module, so both `from kalast.app import App` and
        # `kalast.app.App()` resolve to the live app. Restored afterwards:
        # leaving it in place would break any later `App()` in this process.
        real_app_cls = kalast.app.App
        kalast.app.App = _live_app
        try:
            exec(compile(source, module.__file__, "exec"), module.__dict__)  # noqa: S102
        except BaseException:
            # Printed, not raised: a syntax error in the buffer should show up
            # in the log panel, not take the window down.
            traceback.print_exc()
        finally:
            kalast.app.App = real_app_cls

    return run
