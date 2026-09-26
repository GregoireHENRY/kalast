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
import builtins
import code
import inspect
import io
import keyword
import re
import sys
import traceback
import types
from typing import Any, Callable

import kalast
import kalast.app
from kalast._rs.app._core import console_offer as _console_offer
from kalast._rs.app._core import console_set_more as _console_set_more
from kalast._rs.app._core import console_take as _console_take
from kalast._rs.app._core import console_take_completion as _console_take_completion
from kalast._rs.app._core import console_take_interrupt as _console_take_interrupt
from kalast._rs.app._core import console_write as _console_write
from kalast._rs.app._core import editor_set_python as _editor_set_python
from kalast._rs.app._core import script_write as _script_write

# The namespace of the script running now, for the console to read and write:
# `app` and whatever the script defined. `None` until a script has run.
_script_globals: dict[str, Any] | None = None
# The console's namespace before any script has run.
_idle_globals: dict[str, Any] | None = None
# Its interpreter, made again when the namespace changes.
_console: code.InteractiveConsole | None = None
# Whether the python tab has had the banner `python` greets with.
_greeted = False


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
        # A driven script keeps its own loop here, exactly as it has from a
        # terminal; this only adds what the editor's loop would have done
        # between two frames, and cannot while the script holds it: run the
        # lines typed at the console. Without it the python tab did nothing
        # for as long as a script like `didymos/main.py` was looping.
        app = object.__getattribute__(self, "_app")
        alive = app.step()
        serve_console(app)
        return alive

    # A driven script briefly could not step here at all. The engine's loop used to be entered through
    # `inner.borrow_mut()` held for the whole session, so a script calling
    # back into the app hit an already-borrowed `RefCell`; this class raised
    # instead, blaming the frame. The frame was never the problem -- a script
    # runs between frames -- and the front door now turns `editor_tick` a
    # step at a time so nothing is borrowed while it runs.


class _ScriptStream(io.TextIOBase):
    """`sys.stdout` or `sys.stderr` for a script in the UI app.

    What it writes goes to the log's script tab and on to the terminal, at
    once, with nothing held back in a buffer. Not through descriptor 1: the
    UI app points that at a pipe to catch the engine's own output for the
    kalast tab, and the pipe cannot tell a script's `print` from the engine's
    `println!`. Here, a level up, it can.
    """

    def __init__(self, stream: Any) -> None:
        self._stream = stream

    def write(self, text: str) -> int:
        _script_write(text)
        return len(text)

    def writable(self) -> bool:
        return True

    def isatty(self) -> bool:
        return False

    def fileno(self) -> int:
        # The descriptor underneath, for code that asks: what it writes there
        # lands in the kalast tab, which beats failing.
        if self._stream is None:
            raise io.UnsupportedOperation("fileno")
        return self._stream.fileno()

    @property
    def encoding(self) -> str:
        return getattr(self._stream, "encoding", None) or "utf-8"

    @property
    def buffer(self) -> Any:
        # Bytes written straight to it skip this writer, like `fileno`.
        return self._stream.buffer


def capture_output(app: Any) -> None:
    """Send what a script prints to the log's script tab, and to the terminal.

    Everything else on stdout and stderr -- `H` printing the camera, the mesh
    loader, the `debug_*` flags, the update check, cargo -- is caught in Rust
    a level below, where the UI app points the descriptors at a pipe, and goes
    to the kalast tab. A script's `print` passes through `sys.stdout` before
    it gets there, so it is taken here instead, and the two stay apart.
    """
    # The editor's capture cannot be left to Rust's `Drop`: a callback's
    # globals refer to the app, so the app refers to itself through Python and
    # is never collected. Without this the last line a script prints is lost
    # with the pipe.
    atexit.register(app.flush_output)

    for name in ("stdout", "stderr"):
        stream = getattr(sys, name)
        if not isinstance(stream, _ScriptStream):
            setattr(sys, name, _ScriptStream(stream))

    # The python tab opens as `python` in a terminal does. From here, where
    # the interpreter's version is known, and once.
    global _greeted
    if not _greeted:
        _greeted = True
        # The script editor's language server resolves `import kalast` and
        # numpy against the interpreter the script will actually run in.
        _editor_set_python(sys.executable)
        _console_write(
            f"Python {sys.version} on {sys.platform}\n"
            'Type "help", "copyright", "credits" or "license" for more information.\n'
        )


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
        # The editor's loop is waiting on this one, so lines typed at the
        # console are run here, between this script's frames.
        serve_console(app)
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
    global _script_globals
    module = types.ModuleType("__kalast_script__")
    module.__file__ = path or "<script>"
    module.__name__ = "__main__"
    module.__dict__["kalast"] = kalast
    _script_globals = module.__dict__

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
        global _script_globals
        module = types.ModuleType("__kalast_script__")
        module.__file__ = path or "<editor>"
        module.__name__ = "__main__"
        module.__dict__["kalast"] = kalast
        _script_globals = module.__dict__

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


class _ConsoleStream(io.TextIOBase):
    """`sys.stdout` and `sys.stderr` while a console line runs: the python tab."""

    def write(self, text: str) -> int:
        _console_write(text)
        return len(text)

    def writable(self) -> bool:
        return True


def console_push(app: Any, line: str) -> None:
    """Run one line typed at the log's python tab, as `python` itself would.

    Between frames, so what it changes is drawn by the next one -- which is
    how a paused scene is looked at and moved about. Among the running
    script's variables, `app` one of them; before any script, `app` and
    `kalast`. An expression's value is printed, a line opening a block waits
    for the rest of it, and an empty line closes it: `code.InteractiveConsole`.

    Not something a script calls: the loop does, for each line typed.
    """
    global _console
    namespace = _console_namespace(app)
    if _console is None or _console.locals is not namespace:
        _console = code.InteractiveConsole(locals=namespace)

    _console_write(("... " if _console.buffer else ">>> ") + line + "\n")
    out, err = sys.stdout, sys.stderr
    sys.stdout = sys.stderr = _ConsoleStream()
    try:
        more = _console.push(line)
    except SystemExit:
        # `exit()` would take the window with it, mid-frame.
        _console.resetbuffer()
        more = False
        _console_write("exit() does not apply here: close the window to quit\n")
    finally:
        sys.stdout, sys.stderr = out, err
    _console_set_more(more)


def _console_namespace(app: Any) -> dict[str, Any]:
    """The running script's variables, or before any script `app` and `kalast`."""
    global _idle_globals
    if _script_globals is not None:
        return _script_globals
    if _idle_globals is None:
        _idle_globals = {"app": app, "kalast": kalast, "__name__": "__console__"}
    return _idle_globals


# What ends the word Tab completes -- readline's delimiters. Not `.`, so
# `m.me` is one word: an attribute of `m`.
_DELIMS = " \t\n`~!@#$%^&*()-=+[{]}\\|;:'\",<>/?"
_DOTTED = re.compile(r"([A-Za-z_]\w*(?:\.[A-Za-z_]\w*)*)\.(\w*)$")


def console_complete(namespace: dict[str, Any], line: str) -> tuple[str, list[str]]:
    """What Tab offers for `line`: the part before the word being typed, and
    that word's completions, as Python's own `rlcompleter` would give them.

    Two differences. An attribute's value is not fetched to see whether it
    is callable -- `inspect.getattr_static` says so from the class -- so a
    property is never run: `m.` on a 3.1M-facet mesh would otherwise copy
    its arrays to decide a `(`. And as there, only a dotted name is looked
    into -- `a.b.` but not `f().` -- so a Tab never calls anything.
    """
    start = max(line.rfind(d) for d in _DELIMS) + 1
    head, word = line[:start], line[start:]

    dotted = _DOTTED.match(word)
    if dotted:
        path, prefix = dotted.group(1), dotted.group(2)
        first, *rest = path.split(".")
        try:
            obj = namespace[first] if first in namespace else getattr(builtins, first)
            for name in rest:
                obj = getattr(obj, name)
        except Exception:
            return head, []
        hide = not prefix.startswith("_")
        # The UI app's `app` is a proxy: its own few methods, and the app's.
        names = set(dir(obj))
        if isinstance(obj, (_ScriptApp, _EditorApp)):
            names |= set(dir(object.__getattribute__(obj, "_app")))
        matches = []
        for name in sorted(names):
            if not name.startswith(prefix) or (hide and name.startswith("_")):
                continue
            static = inspect.getattr_static(obj, name, None)
            if static is None and isinstance(obj, (_ScriptApp, _EditorApp)):
                static = inspect.getattr_static(object.__getattribute__(obj, "_app"), name, None)
            call = callable(static) and not inspect.isdatadescriptor(static)
            matches.append(f"{path}.{name}" + ("(" if call else ""))
        return head, sorted(matches)

    if not word or not (word[0].isalpha() or word[0] == "_"):
        return head, []
    hide = not word.startswith("_")
    matches = set()
    for name in [*namespace, *dir(builtins), *keyword.kwlist]:
        if not name.startswith(word) or (hide and name.startswith("_")):
            continue
        value = namespace.get(name, getattr(builtins, name, None))
        call = callable(value) and name not in keyword.kwlist
        matches.add(name + ("(" if call else ""))
    return head, sorted(matches)


def serve_console(app: Any) -> None:
    """Run the lines typed at the console since the last frame, and answer
    a Tab. The loop calls this between frames -- the editor's, or a script's
    own through `step()`."""
    # Ctrl+C at the prompt: the block being typed is dropped, as it is in a
    # terminal. The line itself was never sent.
    if _console_take_interrupt() and _console is not None:
        _console.resetbuffer()
    while (line := _console_take()) is not None:
        console_push(app, line)
    if (line := _console_take_completion()) is not None:
        head, matches = console_complete(_console_namespace(app), line)
        _console_offer(line, head, matches)

