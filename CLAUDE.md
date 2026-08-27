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

## Benchmarking

Two traps, both of which have silently corrupted results here:

- **Build with `--release`.** `maturin develop --release`. Debug is 2-15x
  slower, worst on the per-pixel export loops. Plain `maturin develop` is for
  implementing.
- **Set `app.config.vsync = False`.** Otherwise the loop reports the display
  refresh rate rather than anything about the code — this produced a "3.1M
  facets costs 2x" conclusion that was entirely an artifact.

Also: keep the render window visible and frontmost. macOS throttles occluded
windows, giving runs at 1.8-64 it/s beside siblings agreeing within 1 it/s.
Take medians over repeats and discard the first run after a rebuild.

## Notes

`notes/` holds dated write-ups (`YYYY-MM-DD_topic`). `2026-08-26_TIMELINE.md`
is the running summary, including a list of what is open and what was
deliberately paused.
