# A `.py` runs in the window that is already open

2026-09-21, Mac, after v0.5.1. Reported from the installed bundle:

> I run `kalast examples/crater_self_shadow/step.py` and it launches kalast
> UI app then it closes it and reopen by launching this command in terminal
> `$ …/python/bin/python3 -m kalast examples/crater_self_shadow/step.py`
>
> I said before that i dont want this behavior

Right, and it also contradicts `CLAUDE.md`: *Rust owns the loop and calls
Python for the interpreter, not the other way round.*

## Why it was doing that, and why the reason had expired

The release executable was built `--no-default-features`, so it had no
interpreter in it and could only hand a `.py` to one. The workflow said why:
with the `python` feature pyo3 links libpython **by absolute path** on
macOS, naming a directory that exists on the build machine and nowhere else.

That was true, and it stopped being true earlier the same day, when the
bundle started carrying its own interpreter at a known place relative to the
executable. I did not notice: I improved the spawn's error messages instead
of asking whether it should still exist.

## What it takes

Build the bundled executable **with** the `python` feature, against the
interpreter in the bundle.

| | |
|---|---|
| `PYO3_PYTHON` | the bundle's interpreter, so pyo3 configures against it |
| `-L native=<python>/lib` | python-build-standalone reports `LIBDIR` as `/install/lib`, the path inside the container it was built in, so `-lpython3.14` otherwise resolves to nothing |
| `-Wl,-rpath,@executable_path/python/lib` | libpython's install name is `@rpath/…`, so this is what makes the executable look beside itself instead of at a runner |
| `PYTHONHOME` at startup | a relocated CPython finds its standard library from a prefix compiled in; only `PYTHONHOME` can say otherwise, and only before `Py_Initialize` |

Linux is the same with `$ORIGIN`. Windows has no rpath: it resolves a DLL by
name, searching the executable's directory but not `python/`, so
`python314.dll` is copied beside the executable.

## The part that was not obvious

`abi_fingerprint` hashes `cfg!(feature = "python")`. A host with an
interpreter therefore **forces the guest to have one**, so rebuilding a
`.rs` from a bundle links libpython too. `build_hosted_blocking` configures
that itself:

- `PYO3_PYTHON`, or pyo3's build script introspects whatever `python3` is on
  `PATH` and the guest disagrees with the host;
- the same `-L native=`;
- an rpath of `@loader_path/../../../../python/lib` -- the bundle's
  interpreter counted back from `target/kalast-hosted/python/release/`.
  Relative to the *library*, so a library a release ships keeps working when
  the bundle is moved, which an absolute path baked in on a runner would
  not.

**And `PYTHONHOME` must not be inherited.** It is set in the editor's own
process for the interpreter linked into it; cargo inherits the environment,
pyo3's build script runs a *different* interpreter, and it dies with

```
error: Python script failed
precompiled: 0 up to date, 0 built, 2 failed
```

`cmd.env_remove("PYTHONHOME")` on the cargo child.

## How that one was nearly missed

The local rehearsal reported `2 up to date, 0 built, 0 failed` from inside
the bundle *after* the precompile step had failed -- because `is_current`
compares timestamps, and the bundle had been given dylibs from **9
September** left in `target/` from a fortnight earlier. The script piped to
`tail` without `pipefail`, so the failing exit status went nowhere.

Two things follow. The rehearsal script uses `set -euo pipefail`. And the
CI check has the same blind spot by construction: *"the editor will load
what shipped"* is not *"what shipped was built from this source"*. In the
workflow the precompile step fails the job first, so the hole is covered --
but by accident rather than by the check, which is worth knowing the next
time someone leans on it.

## Verified

`--python-check` starts the embedded interpreter, imports kalast and exits
without a window -- which is how the release workflow can test this at all,
since a runner has no display. Locally, from the bundle layout:

```
embedded interpreter OK: kalast 0.5.1
  sys.prefix …/python
  package   …/python/lib/python3.14/site-packages/kalast/__init__.py
```

and a script run through the editor wrote `pid 73179  frames 60  lit 98.1 %`
from **the editor's own process**, with no `$ … -m kalast` line and nothing
closed or reopened.
