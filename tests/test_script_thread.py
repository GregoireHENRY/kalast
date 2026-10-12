#!/usr/bin/env python
"""The UI app runs its scripts on a thread of their own.

`python -m kalast script.py`, as a user opens it. The script, its callbacks
and the python tab run off the main thread, which goes on turning the loop
-- the window, its panels -- while the script works between two steps.
Each callback is called once a frame, the frame waiting for it; a console
line is served while a driven script loops and while a callback script's
thread is idle; Restart ends a running script at its next step and runs it
again, and what the first run made is freed on the thread that made it;
`close()` from a callback closes the app.

Opens windows, in the background.
"""

import os
import re
import subprocess
import sys
import tempfile
import textwrap

failures: list[str] = []

COMMON = """
import os, sys, threading, time
from kalast.app import App
import kalast.editor as editor
from kalast._rs.app._core import console_submit

app = App()
# `python -m kalast` hands a script the app with `close()` neutralised: the
# editor owns the window. The test closes it all the same.
real = object.__getattribute__(app, "_app")
app.config.open_in_background = True
st = app.simulation.state
st.pause_after_iteration = None
st.is_paused = False

def off_main() -> bool:
    return threading.get_native_id() != threading.main_thread().native_id
"""

DRIVEN = COMMON + """
app.simulation.config.debug.window = True
print("OFF MAIN", off_main(), flush=True)
calls = []
app.before_render = lambda sim, dt: calls.append(("before", sim.state.iteration, off_main()))
app.after_render = lambda sim, dt: calls.append(("after", sim.state.iteration, off_main()))
for _ in range(5):
    app.step()
print("ITERATION", st.iteration, flush=True)
print("CALLS", len(calls), all(c[2] for c in calls), flush=True)
print("SLEEP begins", flush=True)
time.sleep(3.0)
print("SLEEP ends", flush=True)
console_submit("x = 41 + 1")
app.step()
print("CONSOLE", editor._console_namespace(real).get("x"), flush=True)
real.close()
app.step()
"""

CALLBACKS = COMMON + """
frames = []
def after(sim, dt):
    it = sim.state.iteration
    frames.append((it, round(dt, 2), off_main()))
    if it == 1:
        console_submit("y = 6 * 7")
    if it == 4:
        print("CONSOLE", editor._console_namespace(real).get("y"), flush=True)
    if it < 5:
        time.sleep(0.2)
    else:
        print("FRAMES", frames, flush=True)
        real.close()
app.after_render = after
"""

RESTART = COMMON + """
marker = os.environ["KALAST_TEST_MARKER"]
if os.path.exists(marker):
    os.remove(marker)
    for _ in range(3):
        app.step()
    print("SECOND RUN", st.iteration, flush=True)
    real.close()
    app.step()
else:
    open(marker, "w").close()
    n = 0
    while app.step() and n < 200:
        n += 1
        time.sleep(0.02)
        if n == 5:
            real.restart_script()
    print("FIRST RUN ended after", n, flush=True)
"""


def check(name: str, ok: bool, detail: str = "", out: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}" + (f"\n{out[-600:]}" if out else ""))
        failures.append(name)


def run(tmp: str, name: str, source: str) -> tuple[str, int]:
    path = os.path.join(tmp, f"{name}.py")
    with open(path, "w") as fh:
        fh.write(textwrap.dedent(source))
    env = dict(os.environ, KALAST_TEST_MARKER=os.path.join(tmp, "restart.marker"))
    try:
        done = subprocess.run([sys.executable, "-m", "kalast", path], capture_output=True, text=True, timeout=120, env=env)
    except subprocess.TimeoutExpired as e:
        out = (e.stdout or b"").decode() if isinstance(e.stdout, bytes) else (e.stdout or "")
        return out + "\nTIMEOUT", -1
    return done.stdout + done.stderr, done.returncode


def main() -> int:
    tmp = tempfile.mkdtemp(prefix="kalast_script_thread_")

    out, code = run(tmp, "driven", DRIVEN)
    check("driven: the script runs off the main thread", "OFF MAIN True" in out, out=out)
    check("driven: one iteration a step", "ITERATION 5" in out)
    check("driven: both callbacks every frame, off the main thread", "CALLS 10 True" in out)
    # Between the script's two markers the window's loop reports its turns
    # (`debug.window`): it goes on while the script sleeps.
    during = out.split("SLEEP begins", 1)[-1].split("SLEEP ends", 1)[0] if "SLEEP ends" in out else ""
    turns = [int(n) for n in re.findall(r"presents/s, (\d+) frames/s", during)]
    check("driven: the window turns while the script sleeps", len(turns) >= 1 and max(turns) >= 20, f"turns/s {turns}")
    check("driven: a console line is served while it loops", "CONSOLE 42" in out)
    check("driven: closed", code == 0, f"exit {code}")

    out, code = run(tmp, "callbacks", CALLBACKS)
    found = re.search(r"FRAMES (\[.*\])", out)
    frames = eval(found.group(1)) if found else []
    its = [f[0] for f in frames]
    check("callbacks: once a frame, each frame waiting for its own", its == list(range(len(its))) and len(its) == 6, repr(its))
    slow = [f[1] for f in frames[1:5]]
    check("callbacks: dt is the frame's, the callback's 0.2 s in it", all(0.19 <= dt <= 0.4 for dt in slow), repr(slow))
    check("callbacks: off the main thread", bool(frames) and all(f[2] for f in frames))
    check("callbacks: a console line is served between them", "CONSOLE 42" in out, out=out)
    check("callbacks: close() from a callback closes the app", code == 0, f"exit {code}")

    out, code = run(tmp, "restart", RESTART)
    check("restart: the running script ends at its next step", "FIRST RUN ended after 5" in out, out=out)
    check("restart: and runs again", "SECOND RUN 3" in out)
    check("restart: closed", code == 0, f"exit {code}")
    # What the first run made is garbage when the second starts, and was
    # freed by the interpreter's exit on the main thread: each object said
    # so. The scripts' thread frees it as the window closes.
    check("restart: nothing a script made is freed off its thread", "unsendable" not in out, out=out)

    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
