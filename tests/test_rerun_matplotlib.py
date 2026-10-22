#!/usr/bin/env python
"""A script run again in the UI app finds matplotlib as a new process would.

The modules a script imports stay loaded between runs, and pyplot's figure
stayed current with them: run again, `examples/lightcurve_didymos`'s plot
drew its curve over the last run's in the cycle's next colour, and its
`invert_yaxis()` turned the axis back -- upside down every other run. Each
run now starts with no figure and matplotlib's settings as read at start,
the backend aside.

No window: the runners are plain Python, given any object as the app.
"""

import os
import sys

os.environ.setdefault("MPLBACKEND", "agg")

import kalast.editor as editor  # noqa: E402

failures: list[str] = []

SCRIPT = """
import matplotlib
import matplotlib.pyplot as plt
width = matplotlib.rcParams["lines.linewidth"]
matplotlib.rcParams["lines.linewidth"] = 7.0  # a style, left for the next run
plt.plot([0, 1, 2], [0, 1, 0], ".-")
plt.gca().invert_yaxis()
seen = (len(plt.get_fignums()), len(plt.gca().lines), bool(plt.gca().yaxis_inverted()),
        plt.gca().lines[-1].get_color(), width, matplotlib.get_backend().lower())
"""


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def main() -> int:
    import matplotlib

    first = None
    for runner, run in (("run_toplevel", editor.run_toplevel), ("make_runner", editor.make_runner())):
        for i in range(1, 4):
            run(object(), SCRIPT, f"<{runner}>")
            seen = editor._script_globals["seen"]
            first = first or seen
            figures, lines, inverted, color, width, backend = seen
            name = f"{runner}, run {i}"
            check(f"{name}: one figure, one curve", (figures, lines) == (1, 1), f"{figures} figure(s), {lines} curve(s)")
            check(f"{name}: brighter up", inverted)
            check(f"{name}: the cycle's first colour", color == first[3], repr(color))
            check(f"{name}: the last run's style gone", width == first[4], f"linewidth {width}")
            check(f"{name}: the backend kept", backend == "agg", backend)
    check("the first run's linewidth is matplotlib's", first[4] == matplotlib.rcParamsOrig["lines.linewidth"], repr(first[4]))
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
