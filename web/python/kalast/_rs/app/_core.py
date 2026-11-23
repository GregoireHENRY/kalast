"""`kalast._rs.app._core` in the page: the app, and the functions
`kalast.editor` writes the script and python tabs through."""

from kalast._rs._page import App, _fn  # noqa: F401


def script_write(text: str) -> None:
    _fn("script_write", text)


def console_take():
    return _fn("console_take")


def console_write(text: str) -> None:
    _fn("console_write", text)


def console_set_more(more: bool) -> None:
    _fn("console_set_more", bool(more))


def console_take_completion():
    return _fn("console_take_completion")


def console_take_interrupt() -> bool:
    return bool(_fn("console_take_interrupt"))


def console_offer(line: str, head: str, matches) -> None:
    from pyodide.ffi import to_js

    from kalast._rs._page import _b, _call

    _call(_b.py_fn, "console_offer", to_js([line, head, list(matches)]))


def console_greet(version: str, platform: str) -> None:
    _fn("console_greet", version, platform)


def console_submit(line: str) -> None:
    _fn("console_submit", line)


def editor_set_python(path: str) -> None:
    # The page's editor starts no language server to give it to.
    pass
