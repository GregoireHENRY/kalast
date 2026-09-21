# The release bundle carries its own Python

2026-09-21, Mac. Following the v0.5.0 bundle failing on both example kinds
(`2026-09-21` entry in `TIMELINE.md`). The decision this records:

> Kalast **is** the executable. `pip install kalast` is for people who want
> the package, the way `cargo add` is. Someone who downloads a release should
> not have to make a virtualenv, and should not be asked to install anything.

So the bundle ships an interpreter with kalast already in it, and the
executable uses that one before it looks at `PATH`.

## What it costs, measured rather than guessed

A `pip install kalast` into an empty 3.14.3 venv:

| | |
|---|---|
| everything `pyproject.toml` declares | **766 MB** |
| the closure `import kalast` actually reaches | **374 MB** |
| the bundle, unpacked, interpreter + res/ + examples/ + notes/ | **380 MB** |
| the bundle, compressed, per platform | **130 MB** |

The two lists are not the same thing and that is the point. `pyproject.toml`
declares what a *developer* wants available -- pymeshlab (171 MB, imported by
exactly one script, `examples/mesh/decimate.py`), astropy, pandas, symfit --
and none of it is reached by `import kalast`. The bundle installs
`tools/bundle-requirements.txt` instead: numpy, scipy, matplotlib, pyarrow,
spiceypy. `tools/bundle_closure.py` re-derives that list the way it was
derived -- install the wheel `--no-deps`, import, add whatever the error
names, repeat.

**The obvious lever, not pulled yet.** pyarrow (120 MB) and scipy (54 MB) are
174 of the 331 MB of interpreter, and nothing in the engine wants them:
`kalast/__init__.py` eagerly imports `kalast.plot`, and `plot/tool.py` and
`plot/smap.py` import them at module level. A PEP 562 `__getattr__` in
`kalast/plot/__init__.py` would defer both without changing a single call
site, and would take the compressed asset from 130 MB to something near 60.
Worth doing; it is a change to the Python package, so it is its own commit.

## How it hangs together

python-build-standalone, pinned by date (`PBS_RELEASE`/`PBS_PYTHON` in the
workflow) rather than floating -- a bundle that quietly changed Python
between tags is a different product. It unpacks as `python/`, and
`python_beside` in `src/bin/kalast.rs` looks exactly there, relative to
`current_exe` and **not** to the working directory: a bundle is unpacked
wherever the user likes and usually run through a path.

The wheel installed into it is the one *this run* built, pulled from the
`pypi-wheel-<target>` artifact, so `executable` now `needs: wheels`. Using
PyPI instead would be circular on a first tag and would pin the bundle to an
older build on a re-run.

Order of preference when a `.py` arrives: `KALAST_PYTHON` if set (alone --
falling back would hide a typo in it), then the bundled interpreter, then
`python` and `python3`.

## Verified here, end to end

Not "it should work": the bundle was assembled on this machine and run.

```
$ ./kalast _smoke.py
$ …/kalast-v0.5.1-macos-arm64/python/bin/python3 -m kalast _smoke.py
loading model: "res/plane_crater_1024-5000_h=0.437.obj"
SMOKE OK: 60 frames rendered, last lit fraction 98.1 %
```

The executable chose the interpreter beside it, and that interpreter rendered
60 frames with shadows and read facet illumination back off the GPU, on a
machine where `pip install kalast` had never been run.

**And the workflow now runs what it publishes.** Every one of the three v0.5.0
faults would have been caught by a step that imports kalast in the assembled
bundle, and there was no such step. There still cannot be a *render* test --
the runners have no display -- but the import is precisely what failed.

## One trap found on the way

`import kalast` **segfaults** if the working directory is a clone of this
repository. `sys.path[0]` is the working directory, so the repo's own
`kalast/` package wins over the installed one, and its `_rs.abi3.so` is linked
against whichever libpython built it:

```
/Users/gregoireh/.local/share/uv/python/cpython-3.14.3-…/lib/libpython3.14.dylib
```

Load that into a different interpreter and there are two CPython runtimes in
one process. It cost half an hour here, diagnosed only because `otool -L`
named a path in `$HOME` that no release artefact could contain.

Two consequences, both applied: the workflow's smoke step runs from inside
the bundle and never from the checkout, and `run_script` now tells a crashed
import (`ExitStatus::code()` is `None`) apart from a missing package, naming
the shadowing clone as the likely cause. Before, a segfault was reported as
"found, but it has no kalast package".
