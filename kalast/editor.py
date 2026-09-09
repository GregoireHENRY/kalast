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

`step()` is the exception, and is left alone: a script that drives
`while app.step():` keeps driving it. The script runs *between* the editor's
frames, so its loop nests inside the editor's rather than fighting it.
"""

import atexit
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

    # `step` is deliberately *not* overridden: a driven script keeps its own
    # loop here, exactly as it has from a terminal.
    #
    # It briefly could not. The engine's loop used to be entered through
    # `inner.borrow_mut()` held for the whole session, so a script calling
    # back into the app hit an already-borrowed `RefCell`; this class raised
    # instead, blaming the frame. The frame was never the problem -- a script
    # runs between frames -- and the front door now turns `editor_tick` a
    # step at a time so nothing is borrowed while it runs.


def capture_output(app: Any) -> None:
    """Make Python's output reach the editor's log panel promptly.

    The mirroring itself is done in Rust, a level below this: the editor
    points stdout and stderr at a pipe and drains it into the panel each
    frame, then writes it on to the real stdout. That catches the renderer's
    own output as well -- `H` printing the camera, the mesh loader, the
    `debug_*` flags -- which a Python-level tee never saw, since those are
    `println!` straight to the file descriptor.

    What is left to do here is buffering. Python block-buffers stdout when it
    is not a terminal, and it is a pipe now, so `print` would arrive in
    kilobyte lumps long after the fact. Line buffering puts it back.
    """
    # The editor's capture cannot be left to Rust's `Drop`: a callback's
    # globals refer to the app, so the app refers to itself through Python and
    # is never collected. Without this the last line a script prints is lost
    # with the pipe.
    atexit.register(app.flush_output)

    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(line_buffering=True)
        except (AttributeError, ValueError):
            # Not a text stream, or already replaced by something else.
            pass


class _Restart(BaseException):
    """Unwind a running script because another has been asked for.

    `BaseException`, not `Exception`: a script with a broad `except` around
    its loop should not be able to swallow the request and carry on.
    """


class _ScriptApp:
    """The live app, with `step()` watching for a new run request.

    A script that drives its own loop holds it for as long as it wants, so
    Play on a *different* script would otherwise not be seen until the first
    one finished -- which for a long run is never. Checking on each step and
    unwinding hands the loop back to whoever owns it.
    """

    __slots__ = ("_app",)

    def __init__(self, app: Any) -> None:
        object.__setattr__(self, "_app", app)

    def __getattr__(self, name: str) -> Any:
        return getattr(object.__getattribute__(self, "_app"), name)

    def __setattr__(self, name: str, value: Any) -> None:
        setattr(object.__getattribute__(self, "_app"), name, value)

    def step(self) -> bool:
        app = object.__getattribute__(self, "_app")
        alive = app.step()
        if app.script_requested:
            raise _Restart
        return alive

    def start(self) -> None:
        # `start()` owns the loop in Rust and cannot be unwound from Python,
        # so it is run as `while step():` instead -- the same frames, with a
        # check between them.
        app = object.__getattribute__(self, "_app")
        while self.step():
            if not app.running:
                break


def run_toplevel(app: Any, source: str, path: str) -> None:
    """Execute a script as the program, with the editor drawn around it.

    Unlike `make_runner`, nothing is neutralised: the script's own
    `app.start()` or `while app.step():` runs for real and owns the loop. The
    editor is a *mode* the frame draws in, so a driven script gets the panels
    without changing a line -- which is the point of `step()` not blocking.

    `App()` still hands back the live app, so the script does not build a
    second one and lose the editor settings already applied to this one.
    """
    module = types.ModuleType("__kalast_script__")
    module.__file__ = path or "<script>"
    module.__name__ = "__main__"
    module.__dict__["kalast"] = kalast

    proxy = _ScriptApp(app)

    def _live_app(*_args: Any, **_kwargs: Any) -> Any:
        return proxy

    real_app_cls = kalast.app.App
    kalast.app.App = _live_app
    try:
        exec(compile(source, module.__file__, "exec"), module.__dict__)  # noqa: S102
    except _Restart:
        # Another script was asked for. Not an error, and not printed.
        pass
    except BaseException:
        # Printed, not raised: a mistake in the buffer should show up in the
        # log panel, not take the window down.
        traceback.print_exc()
    finally:
        kalast.app.App = real_app_cls


def make_runner() -> Callable[[Any, str, str], None]:
    """Build the callable for `app.script_runner`.

    The app arrives per call rather than being captured here: it has to be a
    handle of the script's own, not the object whose `start_editor()` is
    still on the stack.
    """

    def run(app: Any, source: str, path: str) -> None:
        proxy = _EditorApp(app)

        def _live_app(*_args: Any, **_kwargs: Any) -> _EditorApp:
            return proxy

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
