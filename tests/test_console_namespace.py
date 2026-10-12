#!/usr/bin/env python
"""The python tab's console keeps the names it binds.

It reads the running script's variables, but a name it binds is its own: a
script whose loop set `a` every frame took the console's `a` with it -- a
mesh kept there read as the loop's float a line later, its completions a
float's ("as_integer_ratio( conjugate( ..."). The script's own `a` is left
alone, a name the script binds later shows up, and a name the console
deletes shows the script's again.

No window: the console is plain Python between frames.
"""

import sys

import kalast.editor as editor

failures: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def main() -> int:
    script = {"__name__": "__main__", "a": 0.0}
    editor._script_globals = script
    editor.console_push(None, "a = [1, 2]")
    script["a"] = 0.001  # the script's loop, a frame later
    editor.console_push(None, "a.append(3)")
    ns = editor._console_namespace(None)
    check("the console's a is its own", ns["a"] == [1, 2, 3], repr(ns["a"]))
    check("and the script's is the script's", script["a"] == 0.001, repr(script["a"]))
    _, matches = editor.console_complete(ns, "a.app")
    check("its completions are a list's", matches == ["a.append("], repr(matches))

    script["b"] = 7
    editor.console_push(None, "c = b * 2")
    check("a name the script binds later is read", editor._console_namespace(None).get("c") == 14)
    editor.console_push(None, "del a")
    check("deleted, the script's shows again", editor._console_namespace(None)["a"] == 0.001)

    editor._script_globals = {"__name__": "__main__", "a": 1.5}
    ns = editor._console_namespace(None)
    check("a new run starts the console afresh", ns["a"] == 1.5 and "c" not in ns, repr(sorted(ns)))
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
