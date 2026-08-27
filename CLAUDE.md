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

### Record it, so `git pull` does not undo the work

Write what you learn to **`local_paths.toml`** at the repo root. It is
gitignored, so it never conflicts on pull and is never committed with someone
else's directory layout in it:

```toml
[roots]
spice = "/path/to/spice"
mesh  = "/path/to/mesh"
hera  = "/path/to/hera"

[kernels]
meta = "/path/to/spice/hera/kernels/mk/hera_plan_local.tm"

[notes]
# anything machine-specific worth remembering, e.g. a .tm whose PATH_VALUES
# was edited, or a mesh stored under a different name here
```

Read it at the start of a session and use it instead of re-interviewing. When
a pull brings in examples carrying the author's paths again, re-point them
from this file rather than asking the user to redo the setup.

This is a record, not a config the code reads — the examples still hardcode
their paths. Making them read it would be a real improvement and is not done.

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

Also: keep the render window visible and frontmost. macOS throttles occluded
windows, giving runs at 1.8-64 it/s beside siblings agreeing within 1 it/s.
Take medians over repeats and discard the first run after a rebuild.

## Notes

`notes/` holds dated write-ups (`YYYY-MM-DD_topic`). `2026-08-26_TIMELINE.md`
is the running summary, including a list of what is open and what was
deliberately paused.
