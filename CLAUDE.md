# kalast — working rules

Committed to the repo so it applies on every machine this is cloned to, not
just the one it was written on.

## Rust core, Python wrapper

**The engine is Rust. Python is a binding, not a place to put behaviour.**
Anything a user can do from Python should be a call into Rust that a Rust
program could make just as well. If a feature only works when driven from
Python, it is in the wrong language.

This is not a style preference. It is why kalast is fast, and it is what keeps
a Rust example and a Python script the same program with two front doors.

Two ways it slips, both of which have happened:

- **A loop or a policy written in Python because that was where the caller
  was.** `kalast/__main__.py` grew the editor's whole run loop -- argv
  parsing, the pause and auto-run policy, the rebuild-on-Play cycle -- so the
  editor could not be opened from Rust at all. There is no `[[bin]]`, and
  `App::start_editor()` opens a window whose Play button does nothing, because
  the part that rebuilds lives in Python.
- **A second implementation next to the Rust one.** `kalast/editor.py`
  captures stdout for the log panel; `src/app/gui/mod.rs` has `StdioCapture`
  doing the same thing. Two implementations of one feature, and since the Rust
  one is compiled out on Windows they do not even behave alike.

The honest exception is the interpreter itself: running a `.py` script means
`exec`, which needs CPython. The rule that keeps this from becoming an excuse
is **Rust owns the loop and calls Python for the interpreter**, not the other
way round -- a callback Python registers, not a loop Python runs.

When adding anything to `kalast/*.py`, ask what a Rust caller would do for the
same thing. If the answer is "cannot", the design is wrong.

## After a pull, read the new notes

`git pull` first, then **read every note the pull brought in**, without being
asked. `notes/` is where the reasoning lives -- what was measured, what was
tried and rejected, what is still undecided -- and a session that skips it
repeats work or contradicts a decision made yesterday on the other machine.

    git log --diff-filter=AM --name-only OLD..NEW -- notes/ | grep '^notes/' | sort -u

Read the handoff first if there is one, then the rest. `TIMELINE.md`,
`API.md`, `CONFIG.md` and `CONTROLS.md` are living documents, so a diff is
more useful there than a re-read.

## Destructive commands

Only run `rm -rf` (or `git clean -f`, mass deletes, bulk overwrites) on
directories created in the current session purely as scratch space — something
under `/tmp`, or a directory made for a one-off test.

For anything pre-existing, created by the user, or that another process might
be writing into: **ask first**, even when the intent looks low-stakes ("just
clearing this before a benchmark").

Do not infer safety from a failed command. `rm -rf` is not atomic and can
delete a great deal before failing — a concurrent writer repopulating a
directory mid-sweep makes the final `rmdir` fail *after* files are gone. Check
actual state (`ls`) rather than assuming an error meant nothing happened.

This has bitten once already: `rm -rf out/frames` during a benchmark raced a
live multi-hour export and destroyed frames 0-3922, then errored in a way that
looked like it had done nothing.

`out/` in particular is real output, not scratch.

## First-time setup, for a new user or a new machine

Examples hardcode absolute data paths — twelve of them, under three roots
(`.../spice`, `.../mesh`, `.../hera`) — written for the author's machine. So a
fresh clone will not run until the paths are pointed somewhere real. Walk the
user through this before trying to run anything, and **ask rather than guess**:
these files are large downloads that live wherever the user put them.

1. **SPICE kernels.** Ask for the kernel tree, and which meta-kernel (`.tm`)
   to use. Then **open the `.tm` and check its `PATH_VALUES`** — it is usually
   `'..'`, relative to the `mk/` directory, and if it does not resolve on this
   machine every `furnsh` fails with an error that does not name the real
   cause. Set it to the absolute kernel root; forward slashes work on Windows.
   Keep the pristine original alongside if you edit one.
2. **Shape models.** Ask where the `.obj` meshes live, full-resolution and any
   decimated versions. Several examples want both — a full-res render mesh and
   a 100k `shadow_path` proxy.
3. **`res/`.** Ships with the repo. If it is missing, `README.rst` says to get
   it from cloud-as.oma.be.
4. **Verify, do not assume.** A path existing is not enough. Confirm the
   kernels actually cover the epoch a script uses: `spice.furnsh` then a
   `spkpos`/`pxform` at that time. Coverage gaps surface as `SPKINSUFFDATA`
   much later, mid-run. Then run one example end to end before calling setup
   done.

### Then point the examples at their data — do it for them

Twelve example scripts carry the author's absolute paths, 42 of them across
three roots. Do not hand a new user a list and leave them to it; make the
edits, then show what changed.

```sh
grep -rl "/Users/gregoireh/data" examples --include=*.py | grep -v /old/
```

The three roots and what lives under each:

| Root | Used for | Occurrences |
|---|---|---|
| `.../spice` | meta-kernels (`mk/*.tm`) and DSK shape models | 18 |
| `.../mesh` | `.obj` shape models, full-res and decimated | 21 |
| `.../hera` | TIRI image lists and instrument response CSVs | 3 |

Work one root at a time and re-run the grep after each, so nothing is missed.
A path may not map one-to-one: the same mesh can sit under a different
filename, or under `spice/.../dsk/` on one machine and `mesh/` on another.
Ask when a target is ambiguous instead of picking one.

After editing, **run the script**. A wrong path fails immediately and clearly;
a path that exists but points at the wrong file, or at kernels that do not
cover the epoch, fails much later and confusingly. `python tests/test_stubs.py`
needs no external data, so it is the right first check that the build itself
works, before anything data-dependent: it builds a mesh from scratch and
compares the compiled module against its stubs. It does want a GPU adapter,
since it constructs an `App`.

Start the user on an example that needs the least: `examples/cube/light.py` and
`examples/two_spheres/main.py` use only `res/`, so they run on a fresh clone
with no data paths at all. Use those to confirm the renderer works before
touching a Hera script.

## Building

- **`maturin develop`** (debug) while implementing a feature — fast to
  rebuild, and the only thing you want during the edit/run loop.
- **`maturin develop --release`** once the feature works, and for **every**
  benchmark or real data run. Not just timing work: any run whose output you
  intend to keep or publish.

**On Windows, build with `python tools/develop.py`** rather than calling
maturin directly. A mapped DLL cannot be written on Windows, and VS Code's
language server keeps `kalast/_rs.pyd` mapped for the whole editor session, so
plain `maturin develop` throws the finished build away at the copy step with
`os error 32`. The wrapper renames the old module aside first -- renaming a
mapped image *is* allowed -- and passes everything through, so
`python tools/develop.py --release` works too. macOS does not need it and is
unaffected either way.

**Python is a default feature.** `cargo build` links pyo3 and, for the binary,
an interpreter -- which is what lets `cargo run --bin kalast` run a `.py` in
its own window. For the engine alone, `--no-default-features`; that is also
the build to use where no Python install is available.

**The Python module and a Rust example are two separate builds.** `maturin
develop` updates what `python -m kalast` and every `.py` run use; `cargo build
--release --example <name>` updates the example binary. A change to shared
code needs both, and neither command warns that the other is stale -- which
looks exactly like the change not working. The editor's `compile` button
covers the example side for you; from a terminal, remember both.

Debug is 2-15x slower here, worst on the per-pixel frame-export loops
(measured 22.6 -> 53.1 it/s at 3.1M facets with export on), so a debug data
run wastes hours for nothing.

Nothing else is worth adding to the release profile: `lto = "fat"` +
`codegen-units = 1` were measured and gave no improvement while pushing the
build from ~59 s to ~87 s. Recorded in `Cargo.toml` so it is not retried.

## Test and benchmark runs

**Never run against the project's real output directories.** Frame export
defaults to `out/frames`, so a benchmark left at the default writes into
whatever a real run is using, and two exporters pointed at one directory race
on the startup index scan as well as on cleanup. Always redirect:

```python
app.config.export_dir = "/tmp/<something>/frames"
```

**Put throwaway scripts under `/tmp`, not in the repo.** Copy the example,
instrument the copy, run it from there. That keeps `examples/` clean, and it
means the scratch directory is one this session created and may therefore
delete without asking (see above) — which the project's own directories are
not.

Do not benchmark by editing an example in place: the shortened sweep and the
rate prints are not changes anyone wants committed.

## Benchmarking

Beyond building `--release` (above), one trap has silently corrupted results
here more than once:

- **`app.config.vsync` must stay `False`.** Otherwise the loop reports the
  display refresh rate rather than anything about the code — this produced a
  "3.1M facets costs 2x" conclusion that was entirely an artifact of a 120 Hz
  panel. It is the **default** since 15 September, so this is now a matter of
  not switching it on rather than remembering to switch it off; scripts that
  still set it explicitly are harmless.

Take medians over repeats and discard the first run after a rebuild.

**A Rust example and the Python module do not compare end to end** -- not yet.
A `cargo` example binary sits pinned at the panel's refresh rate whatever the
workload, while the extension module does not, with the same adapter, the same
surface size and `Immediate` granted to both. Compare the loop *body* if you
want a language number; see `notes/2026-09-08_step_one_frame_and_a_bad_benchmark.md`.

**The occluded-window rule is gone; the cause was a bug, now fixed.** This
file used to say to keep the render window visible and frontmost, because
occluded runs came in at 1.8-64 it/s beside siblings agreeing within 1 it/s.
That was not macOS throttling. `get_surface_texture` returns `Occluded` when
the window is covered, and the frame handler took that as a reason to `return`
before running `before_render`, `after_render` or `simulation.update()`. An
occluded window did not run slowly, it **stopped**: no steps, no iterations,
wall time still accruing.

The frame now runs without a surface and skips only the blit and the present,
so a run behind another window proceeds at full speed. Measured on the
view-factor cadence sweep: stalled at 38 rebuilds for 18 minutes before,
30 rebuilds in 3 minutes after.

What this means for old numbers: **results are unaffected** -- a run that
finished took the same steps and the same physics, since the skipped frames
did no work at all -- but **any timing taken while focus was lost is too slow**,
never too fast. Historic it/s figures in `notes/` are lower bounds if the
window was covered.

## Python type stubs

Editors cannot complete a compiled extension: `app.simulation.config.grid.<tab>` offers
nothing unless a `.pyi` says what is there. The stubs under `kalast/**.pyi`
are **generated from the Rust source**, not hand-written:

```sh
python tools/gen_stubs.py            # after changing any #[pyclass]
python tests/test_stubs.py           # checks they are current
```

The check runs two ways and both must pass: the committed stubs must match
what the generator produces, *and* every class must match `dir()` on a real
object, which catches the generator itself misreading an attribute.

This is generated rather than written because the repo already carried
hand-written `.pyi` files that had been **commented out entirely** -- so they
completed nothing while looking like the surface was covered, which is worse
than having none.

## The editor's config panel

Same story as the stubs, same reason: `src/app/gui/config_panel.rs` is
**generated** from `src/app/config.rs`, not written by hand.

```sh
python tools/gen_config_panel.py     # after adding a field to Config
python tests/test_config_panel.py    # checks it is current and complete
```

Two guards, both needed: the committed file must match what the generator
produces, *and* every field must appear in it. The second catches the case
that matters -- a new option with no widget is invisible, because the panel
still looks complete.

**The struct's nesting is the grouping.** `Config` is a struct of sub-structs
-- `shading`, `light`, `shadows`, `grid`, ... -- and the generator emits one
function per group. There is no prefix table and no `:group:` marker: to move
a field between headers, move it between structs, and the panel, the Python
surface and the docs all follow. `src/app/gui/simulation_panel.rs` composes
those functions under topic headers by hand, each beside the entity it
describes -- the Sun's position and the Sun's colour under one header. Which
groups share a header is decided there and nowhere else.

The Rust doc comments carry what the type cannot:

| marker | effect |
|---|---|
| `/// :range: 0..=16` | a slider with those bounds instead of a drag field |
| `/// :step: 0.01` | drag speed |
| `/// :skip:` | no widget; for things edited from a script, like `colormap` |
| `/// :py_custom:` | no generated Python accessor; see the next section |

Otherwise the widget follows the type, and the first sentence of the doc
becomes the hover text -- so documenting a field in Rust documents it in the
UI.

## The Python getters, generated too -- and the trap that made them so

The `#[getter]`/`#[setter]` pairs were the one mirror of the config still
written by hand, and it showed: a field could be complete everywhere -- widget
in the editor, line in the `.pyi` -- and still raise `AttributeError` from a
script, which the stubs cannot catch because they are generated *from* the
wrapper. That bit four times (`debug_light_cube_fit`, `facet_labels`,
`selection_color`, `colorbar_border`). Since 15 September:

```sh
python tools/gen_bindings.py            # after adding a field to Config
python tests/test_config_bindings.py    # every field reachable and writable
```

`src/py/app/config_gen.rs` holds one view class per group, each carrying the
same `Rc<RefCell<Config>>` as the root and reading its own group through it.
**That indirection is why these are generated rather than being
`#[pyclass(get_all)]` on the sub-structs**: a `get_all` getter hands Python a
*copy*, so `config.grid.color = ...` on a copy sets nothing, silently. Every
accessor has to go through the shared handle, which is a page of identical
code per group -- exactly the thing to generate.

A field with real logic opts out with `/// :py_custom:` and is written by hand
in `src/py/app/config.rs` -- today only `data.colormap`, which parses names
and arrays. `tests/test_config_bindings.py` still checks those, since a
forgotten hand-written accessor is the old bug back.

Old flat names -- `config.grid_color`, `config.vsync` -- keep working for one
release through a `__getattr__`/`__setattr__` shim on the root, generated from
`tools/config_renames.py`, with a `DeprecationWarning` naming the new path.
Three that moved to `app.config` (`title`, `fullscreen`, `vsync`) raise an
error saying so instead.

## Notes

`notes/` holds dated write-ups (`YYYY-MM-DD_topic`). Two are **undated on
purpose**, because they are living documents rather than a record of one day:

- `TIMELINE.md` — the running summary, including what is open and what was
  deliberately paused.
- `CONFIG.md` — the `app.config` reference. Add an entry here whenever
  a config option is added, or it goes stale silently.
- `CONTROLS.md` — keyboard and mouse bindings for the render window. Same rule:
  add to it whenever a binding is added.
- `API.md` — the Python API outside the config: `App`, `sim.state`,
  `sim.huds`, bodies, camera and Sun, and the GPU-result queries. Same rule
  again: add to it whenever something is exposed to Python.

## Releasing

The GitHub release text is `CHANGELOG.md`'s section for that version,
verbatim, and nothing else: a list of what changed **for the people who use
kalast**, no prose. Release engineering, CI, caching, and refactors that
change nothing a user sees do not go in it -- those belong in `notes/`. The version
gate refuses a tag whose section is missing or has no `- ` entry.

The order is: bump the three manifests, write the section, push, rehearse
(`gh workflow run release.yml`), and if green, **show the section to the
user and get it approved before pushing the tag** -- every tag, not just the
first. A green rehearsal is the tag's technical go; the changelog review is
its editorial one, and it is the user's, not yours. A tag whose commit was
rehearsed publishes the rehearsal's artefacts, so the tag run is minutes.

