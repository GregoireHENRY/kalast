# The editor runs both kinds of example, in its own window

Where this started: opening a Rust example in the editor showed the source and
a black viewport, and pressing Play closed the window and opened another. The
Python side had none of that — a `.py` ran in the window you were looking at.

It does now for both, from both front doors:

| | `.py` | `.rs` |
|---|---|---|
| `python -m kalast` | in this window | in this window |
| `cargo run --bin kalast` | in this window | in this window |

Nothing is written in an example to make that true. `examples/crater_self_shadow/step.rs`
is a program — `fn main`, an `App` of its own, `while app.is_running()` — and
`cargo run --release --example crater_step` still runs it from a terminal.

## Hosting a Rust example

The editor generates a wrapper crate under `target/kalast-hosted/`, compiles
the example into a **cdylib** through it, loads it with `libloading`, and
calls the example's `main`.

Three things make it *this* window rather than a second one.

**The host drives the loop.** The guest is its own copy of the crate, with its
own copy of winit's state. `step`, `start`, `close` and `is_running` detect
that they are hosted and cross back to the host through function pointers
(`src/app/hosted.rs`). A guest pumping the event loop from its own copy would
be a second winit talking to one platform.

**The host adopts the guest's scene.** `App::new()` in the example builds a
real app; the simulation and the callbacks are handed over on the first call
that needs a frame. The rest of that app was scaffolding — a window it never
opened.

**Loading happens between frames.** An example's `main` may call `step()`, and
stepping from inside a frame re-enters the event loop. That killed the process
the first time it ran. `editor_tick` takes the request after a frame ends,
which is the same place a Python script runs, and for the same reason.

### The wrapper copies the example

Two other ways were tried first:

- `#[path] mod example;` puts `fn main` out of reach, because an example's
  `main` is private and privacy does not reach outwards;
- `include!` cannot carry a file whose header is `//!`, since inner attributes
  may not come from a macro expansion — and every example here starts with one.

Copying costs a rewrite of those header lines to `//` and buys a file where
`main` is an ordinary private function beside the exports.

### Restart, on something that owns the flow

A driven example does not return until its loop ends, so Restart had nothing
to act on. `step()` and `is_running()` now report the run over once another
load is pending, and the example's own `while` ends the way it ends when the
window closes.

Unwinding it with a panic was tried and is wrong twice over: a panic crossing
`extern "C"` aborts, and the sentinel's `TypeId` differs between the two
copies of the crate, so the host cannot recognise its own payload. It
re-raised it and took the process down.

## Hosting a Python script from Rust

`cargo run --bin kalast foo.py` used to spawn `python -m kalast foo.py` and
close. Now the binary embeds an interpreter and calls
`kalast.editor.run_toplevel` — the same call `python -m kalast` makes, against
the same app.

It registers **its own** bindings through `append_to_inittab` rather than
letting the interpreter import the installed `kalast/_rs`. Otherwise there
would be two copies of the engine in one process, the script's `App` in one
and the window in the other, and the script would configure a simulation
nothing draws.

Two things had to give:

- the embedded interpreter's `sys.path` is the *base* interpreter's, with
  neither this repository nor the environment kalast's dependencies live in;
  both are prepended before anything is imported;
- `kalast/__init__.py` did `del _rs`, assuming it had been bound as a package
  attribute — which happens on a real submodule load, not when the bindings
  arrive through `sys.modules`.

### Python became a default feature

An interpreter in the process is the only way to run Python, and that is the
dependency `notes/2026-09-09_rust_core_audit.md` had just removed. So it is a
**default** feature rather than a mandatory one, and the audit's property
survives as an opt-out:

```sh
cargo build --no-default-features        # engine alone, no pyo3, no libpython
cargo test  --lib --no-default-features  # 42 tests, no Python install
```

Built that way the binary hands a `.py` over as before, and says so.

## Keeping the two sides honest

Passing an `&mut App` across a library boundary is sound only while both sides
are the same crate built the same way. Rust has no stable ABI, and this
crate's `python` feature genuinely changes layout — it adds a field to
`Shared` and a variant to `Tick`.

Two checks, because one was not enough:

- **`abi_fingerprint`**, exported by the guest and checked before anything is
  called. It caught a library built without `python` against a host with it,
  as a message rather than as corruption.
- **A timestamp against the engine.** The fingerprint covers sizes and the
  offsets of the fields reached across the boundary, and even so a `bool`
  added to `Shared` fit in existing padding, changed neither, and a library
  from before that change loaded happily — running the old `hosted::step`
  against the new host. The library is now also required to be newer than
  `src/`, `shaders/` and the manifest.

A stale library rebuilds itself rather than asking for a button to be pressed.

## What is still true of the shapes

A hosted Rust example may own a loop or install callbacks; both work.
`main.rs` is the callback twin of `main.py`, `step.rs` the driven twin of
`step.py`, and each runs standalone and hosted.

The one asymmetry left: a `.py` opened through the panel's `open` button does
not start running, while a `.rs` does. A `.rs` has nothing else to show; a
`.py` may be replacing a run that is still on screen.
