# kalast — working rules

Committed to the repo so it applies on every machine this is cloned to, not
just the one it was written on.

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
cover the epoch, fails much later and confusingly. `examples/mesh/simple.py`
needs no external data, so it is the right first check that the build itself
works, before anything data-dependent.

Start the user on an example that needs the least: `examples/cube/main.py` and
`examples/two_spheres/main.py` use only `res/`, so they run on a fresh clone
with no data paths at all. Use those to confirm the renderer works before
touching a Hera script.

### Record it, so `git pull` does not undo the work

Those edits are local modifications to tracked files, so every pull that
touches an example will conflict, and `git status` will always look dirty.
That is expected — do not offer to commit them, they are one machine's layout.

Write what was decided to **`local_paths.toml`** at the repo root, gitignored:

```toml
[roots]
spice = "/path/to/spice"
mesh  = "/path/to/mesh"
hera  = "/path/to/hera"

[kernels]
meta = "/path/to/spice/hera/kernels/mk/hera_plan_local.tm"

[notes]
# machine-specific gotchas worth remembering, e.g. a .tm whose PATH_VALUES
# was edited, or a mesh stored under a different name here
```

Read it at the start of a session. When a pull reintroduces the author's
paths, re-apply from this file — a mechanical substitution, no interview
needed. If a conflict is only about these paths, resolving in favour of the
incoming version and re-applying the substitution is usually cleanest.

This is a record, not a config the code reads. Making the examples read it
instead of hardcoding would remove this whole problem, and is not done.

## Building

- **`maturin develop`** (debug) while implementing a feature — fast to
  rebuild, and the only thing you want during the edit/run loop.
- **`maturin develop --release`** once the feature works, and for **every**
  benchmark or real data run. Not just timing work: any run whose output you
  intend to keep or publish.

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

- **Set `app.config.vsync = False`.** Otherwise the loop reports the display
  refresh rate rather than anything about the code — this produced a "3.1M
  facets costs 2x" conclusion that was entirely an artifact of a 120 Hz
  panel.

Take medians over repeats and discard the first run after a rebuild.

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

## Notes

`notes/` holds dated write-ups (`YYYY-MM-DD_topic`). Two are **undated on
purpose**, because they are living documents rather than a record of one day:

- `TIMELINE.md` — the running summary, including what is open and what was
  deliberately paused.
- `CONFIG.md` — the `app.config` reference. Add an entry here whenever
  a config option is added, or it goes stale silently.
- `CONTROLS.md` — keyboard and mouse bindings for the render window. Same rule:
  add to it whenever a binding is added.
