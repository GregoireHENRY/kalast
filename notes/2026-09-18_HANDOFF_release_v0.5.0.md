# Handoff: v0.5.0 is tagged, and the release run is half red

Written mid-run, on the Mac, 2026-09-18. `git pull` and read this first.

## Where it stands

`v0.5.0` is pushed and points at `b4429b0`. The release workflow ran as
`35374536810`:

| | |
|---|---|
| ✅ `the tag is publishable` | the version gate passed against both live registries |
| ✅ `executable` ×4 | linux-x86_64, macos-arm64, macos-x86_64, windows-x86_64 all built |
| ✅ `sdist`, ✅ `wheel macos-arm64` | |
| ❌ `wheel linux-x86_64` | failed in `PyO3/maturin-action@v1` |
| ❌ `wheel windows-x86_64` | failed in `PyO3/maturin-action@v1` |
| ❌ `publish to crates.io` | failed in `Publish kalast_macros, if that version is new` |

**Nothing was published.** crates.io still tops out at 0.4.1 and `kalast` is
still unclaimed on PyPI, so no version has been burned: the run can be fixed
and re-run against the same tag.

I could not read the job logs before handing over -- `gh run view --log` was
returning nothing, probably still uploading. **Start there:**

```sh
gh run view 35374536810 --json jobs -q '.jobs[] | "\(.conclusion)\t\(.name)"'
gh run view --job=105696043420 --log   # crates.io
gh run view --job=105695993789 --log   # linux wheel
gh run view --job=105695993738 --log   # windows wheel
```

## What I would check first, in order

These are hypotheses, not diagnoses. I had not seen a log when I wrote them.

**1. crates.io -- almost certainly the missing token.** The setup was never
done: the workflow reads `secrets.CARGO_REGISTRY_TOKEN` and the tag went up
minutes after I listed it as a prerequisite. crates.io → Account Settings →
API Tokens → New Token, scope `publish-update` (both crates exist and
`GregoireHENRY` owns both -- I checked), then GitHub → Settings → Secrets and
variables → Actions → `CARGO_REGISTRY_TOKEN`.

**2. PyPI -- the trusted publisher is probably not registered either.** The
`pypi` job never ran (it needs the wheels), so this is untested. Register a
*pending* publisher at `pypi.org/manage/account/publishing/`: project
`kalast`, owner `GregoireHENRY`, repo `kalast`, workflow `release.yml`,
environment `pypi`.

**3. The two failing wheels.** `wheel macos-arm64` succeeded and
`wheel macos-x86_64` was still running, so this is not the build itself --
it is platform-specific.
- *Linux*: the manylinux container is a bare CentOS-alike and needs X11,
  Wayland and Vulkan headers for winit and wgpu. My `before-script-linux`
  tries `yum` then falls back to `apt-get`; the package names may be wrong
  for whichever image maturin-action picked, and `vulkan-headers` in
  particular may not exist there. Read the log for the first missing header.
- *Windows*: `before-script-linux` does not apply, so it is something else
  entirely. Suspect the toolchain or Python 3.14 on the runner rather than
  system libraries.

If the wheels prove awkward, note that **the executables all built** -- the
GitHub release half of this works. The wheels are only needed for PyPI.

## Re-running

The workflow is idempotent for everything except an actual publish, and the
version gate refuses a version that is already on a registry, so a re-run
after a partial publish is safe.

```sh
gh run rerun 35374536810 --failed     # after fixing the cause
```

A code fix means a new commit, and the tag would have to move:

```sh
git tag -d v0.5.0 && git push origin :refs/tags/v0.5.0
git tag -a v0.5.0 -m "v0.5.0" && git push origin v0.5.0
```

Moving a tag is only safe *because nothing published*. Once crates.io or PyPI
has 0.5.0, the next attempt is 0.5.1.

## What v0.5.0 contains

The day's work, all measured, all in `notes/2026-09-18_memory_meshes_and_shadow_maps.md`
and the `TIMELINE.md` entries above it:

- **The mesh made one thing.** A flat mesh is the file's shared vertices plus
  a flag, not a second copy of the geometry. Per 3M-facet model: 480 → 187 MB
  on the CPU, 866 → 213 MB on the GPU. The Didymos pair: peak RSS 4.74 → 1.43
  GB, the frame that uploads a mesh 261 → 45 ms, a load 1.01 s → 142 ms.
  `_vertices_before_flatten` and `_indices_before_flatten` are gone.
- **PCF filters rather than moves** (`2026-09-17_pcf_erosion.md`), the Sun can
  sit at its true distance, the shadow array is allocated per body.
- **Blender's trackpad map**, the turntable reversing upside down, the camera
  levelling when you take hold of it (`2026-09-17_trackpad_gestures.md`).
- `sim.state.rate_limited` / `rate_limit`, `app.config.panels_folded`,
  Restart reaching a driven script, the unsaved-changes prompt.

## Two loose ends unrelated to the release

- **`examples/didymos/main.py` now loads the `_100k` meshes** (committed with
  this handoff). That is a 64× cut in geometry from the full models and takes
  the example from 39 to ~220 it/s in the editor. The full models are for data
  products; see the measurements in the memory note. This was the answer to
  "why is it 40 it/s now" -- it was never a regression, the example had been
  switched to the full-resolution models in `9d40269`.
- **`shadow_path` is unexplored and probably the next win.** The shadow pass
  draws every body into every layer, so of ~19 M triangles a frame, 12.6 M are
  shadow casters. A 100k proxy through `load_mesh(shadow_path=...)` would cut
  that without touching the rendered mesh. Nobody has measured it.
