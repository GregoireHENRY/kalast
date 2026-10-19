# Where "Rust core, Python wrapper" is not held

An audit of the codebase against the rule now written down in `CLAUDE.md`,
prompted by finding the editor's run loop in `kalast/__main__.py`. That one is
fixed; these are what is left, worst first.

Not everything Python is a violation. `kalast/plot/*` is matplotlib and
`kalast/spice.py` wraps spiceypy — both are Python libraries, and calling them
from Python is the only sensible thing to do. What follows is code that has no
such excuse.

## 1. The CPU thermophysical model is written in Python

The largest and the most surprising, given that the TPM is what kalast exists
to compute.

| module | lines | calls into Rust |
|---|---|---|
| `kalast/tpm/heating.py` | 471 | 0 |
| `kalast/tpm/explicit.py` | 362 | 0 |
| `kalast/tpm/implicit.py` | 345 | 0 |
| `kalast/tpm/routine.py` | 201 | 0 |
| `kalast/tpm/nonuniform.py` | 131 | 0 |

1,510 lines of numerics, none of it reaching the engine.
`kalast.tpm.routine.step_conduction` — the inner step of the model — is a
Python function, confirmed by `inspect.isfunction`. `src/tpm/routine.rs`
exists but exposes only `update_thermal_state`; it is not the same code.

So there are two thermophysical models: a **GPU one in Rust**
(`src/tpm/gpu.rs`, `shaders/tpm.wgsl`) and a **CPU one in Python**. The
measured 23x is the gap between them, and it is usually read as "the GPU is
fast". Part of it is that the thing being compared against is numpy.

The CPU path is not dead code. `examples/hera_didymos/tpm.py` selects it with
`BACKEND = "cpu"`, and it is the reference the GPU path was validated against
(agreement to 1.5e-05 K). A reference implementation is exactly the thing that
should be in the same language as the engine, so that "the CPU path" and "the
GPU path" differ in where they run and not in what they are written in.

**Worth doing, and large.** The honest note is that porting it is not a
weekend: `heating.py` alone carries the view-factor radiosity with a sparse
matvec per bounce, and `scipy.sparse` has no drop-in equivalent.

## 2. The engine cannot be built without Python — FIXED

Fixed in `0e77033`. `python` is a cargo feature now, turned on by maturin.

**Later made a *default* feature**, so that `cargo run --bin kalast` can run a
`.py` example in its own window -- which needs an interpreter in the process,
and there is no way round that. The property this finding was about survives
as an opt-out rather than a default: `--no-default-features` builds the engine
and the binary with no pyo3 and no libpython, and `cargo test --lib
--no-default-features` runs without a Python install. Being a default feature
rather than a mandatory dependency is exactly what keeps that possible. `cargo run --bin kalast` opens the editor with Python removed
from PATH entirely, and `cargo test --lib` runs without any PATH juggling.

Two pyo3 details decided the shape, and are worth knowing before touching it
again: `#[pyfunction]` and `#[pymethods]` had to become `cfg_attr` rather than
`cfg`, because the engine calls several of those functions itself and gating
the *item* deleted them. Field attributes cannot be done that way at all --
`#[pyclass]` expands before the `#[pyo3(get, set)]` on its own fields, so a
`cfg_attr` there is still unexpanded when the class macro reads it. That is
why `src/routines/` is gated as a whole module rather than field by field.

The original finding follows.

### Original finding

`pyo3` is an unconditional dependency, and the bindings are not confined to
`src/py/`. Eight core modules carry `#[pyclass]`/`#[pymethods]`:

    src/tpm/core.rs        14        src/tpm/properties.rs   8
    src/routines/setup.rs  12        src/tpm/emit.rs         7
    src/mesh.rs            10        src/app/mod.rs          2
    src/math.rs             9        src/tpm/routine.rs      1

This is the rule inverted: the Rust core depends on Python, rather than Python
depending on the core. Two things it costs, both measured today:

- `cargo run --bin kalast` **exits with STATUS_DLL_NOT_FOUND** unless Python's
  DLL is on `PATH`, because the binary links pyo3. A Rust-only entry point
  that needs a Python install to start is not a Rust-only entry point.
- `cargo test --lib` fails the same way, before running a single test. That is
  why the test suite looked broken on this machine.

**The fix is a feature gate**: `pyo3` optional, the `#[pyclass]` attributes
behind `#[cfg(feature = "python")]`, and the module tree under `src/py/` gated
whole. `maturin` enables it; `cargo build` does not. Mechanical, but it
touches every one of those eight files, and the derives on shared structs are
the fiddly part.

Until then, both commands need:

    $env:PATH = "$(python -c 'import sys; print(sys.base_prefix)');$env:PATH"

## 3. Two implementations of stdout capture

`kalast/editor.py:71` `capture_output` and `src/app/gui/mod.rs`
`StdioCapture` do the same job for the same panel. They are not equivalent:
the Rust one is compiled out on Windows (no `dup2` on descriptor 1), so which
one is doing the work depends on the platform, and only the Python one works
here.

Smallest of the three to resolve, and the one most likely to bite: a log panel
that captures different things on different machines is a debugging tool that
lies.

## What was fixed

Both of the entry-point problems. The bindings are a feature (2, above), and
`App::run_editor` is now the engine's, and both `python -m kalast` and
`cargo run --bin kalast` call it. `__main__.py` is 46 lines: create the app,
wire capture, hand over the interpreter callback. The seam is `run_script`,
because `exec` genuinely needs CPython — Rust owns the loop and calls out for
the interpreter, rather than the reverse.
