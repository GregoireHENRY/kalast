# Changelog

Each version's section is the text of its GitHub release, verbatim and
nothing else, and it is written for the people who *use* kalast: what
changed for them. Release engineering, CI and internal refactors do not
belong here. The version gate refuses a tag whose section is missing or has
no entries, and the section is approved by a person before the tag is
pushed. `## Unreleased` collects the entries between versions; the bump
renames it.

## v0.5.5

- Rendering is about 2.5× faster on full-resolution meshes: the Didymos pair at 3.1 M facets each runs `examples/didymos/main.py` at 105 it/s, from 42. The shadow and main passes draw shared vertices instead of expanded corners, closed meshes cull their back faces in the shadow map, and a body is only drawn into the shadow layers it can reach. Nothing in the physics or the shadows changes.
- `shadows.resolution` defaults to 4096 (was 8192): a quarter of the memory per shadow layer and a faster frame, with the per-facet shadowing the thermophysical model runs on unchanged. Set 8192 in a script that wants the finer texel.
- A window opened with `app.config.open_in_background = True` now stays behind your other windows and never takes the keyboard or the mouse; on macOS the process runs without a Dock icon for that run.
- The screen no longer paces the loop. A visible window used to cap the simulation at about two iterations per refresh of whichever display it was on -- 120 it/s exactly on a 60 Hz monitor, 300 on the laptop panel for a light scene. The window is now shown at its display's refresh rate while the simulation runs as fast as the CPU and GPU allow: the same light scene reads 2,850 it/s with 120 frames a second on screen. `step()` also no longer runs ahead of the GPU by more than two frames, so the iteration it returns from is at most two frames from what is drawn.
- The UI app's `open` button opens the system's file picker, starting in the current script's folder or else in `examples/`, showing `.py` and `.rs`; typing a path still works.
- Windows: double-clicking `kalast.exe` no longer opens a console window beside the UI app; the log panel is where its output goes. Started from a terminal, it still prints there.
- `rate_limit` caps the frame rate, in fps: the frame waits for its turn, and since one step is one frame the iteration rate is the same number. It used to hold the counter while frames ran free, which gave an it/s beside an fps.
- The UI app's toolbar shows the iteration and the frame rate; the `it/s` figure, the same number in any run, is gone from the default (`app.config.toolbar` still accepts `{its}`), and the iteration count is gone from the right panel's Run section, where it duplicated the toolbar's.

## v0.5.4

- Rust examples compile from the editor's buffer: edit a `.rs` in the UI app and hit Play or Compile, no Save needed — the same as a `.py` has always worked.
- `res/README.md` explains how to get ESA's Hera SPICE dataset (HERA.zip, 1.1 GB), which holds the kernels and the shape models, and how to set `PATH_VALUES` in a meta-kernel so it loads from any working directory.
- `examples/didymos/main.py` loads the full-resolution Didymos and Dimorphos models; `examples/hera_didymos/afc.py` and `afc_eclip_didy.py` load the `_100k` ones. The files are named `g_01165mm_spc_didy_v003[_100k].obj` and `g_00243mm_spc_dimo_v004[_100k].obj`; if yours still carry the dataset's long names (`g_01165mm_spc_obj_didy_0000n00000_v003.obj`, …), rename them or point the scripts at them.

## v0.5.3

- Opening a `.obj` from the command line frames it: camera and Sun are placed from the mesh, with Blender axes and wireframe on. Also `app.simulation.frame_all()`.
- `import kalast` no longer loads matplotlib, scipy and pyarrow; `kalast.plot` and `kalast.tpm` import on first use. pyarrow is no longer in the bundle.
- The bundle's interpreter is pruned to what the UI app uses; archives are 90–134 MB, from 149–217.
- A `.rs` rebuilds from a bundle whose path contains a space.
- Cargo features: `python` is the bindings alone; `embed` (the default) adds an interpreter for a binary; `ext` builds an extension module. `cargo add kalast --no-default-features --features python` now gets neither link policy.
- `pyproject.toml` is no longer shipped in the bundle.
- "the editor" is the kalast UI app in docs and release notes.

## v0.5.2

- Handing the UI app a `.py` runs it in the window already open; it no longer closes and relaunches through `python -m kalast`. The executable carries an interpreter.
- Windows: the executable ships with the DLLs it needs beside it.

## v0.5.1

- The release bundle carries its own Python with kalast installed: a `.py` example runs with nothing installed.
- Rust examples ship precompiled, and can be rebuilt from a bundle — against crates.io, with a toolchain fetched into the bundle folder if the machine has none.
- Clear errors when a `.py` or `.rs` cannot run from a bundle; `shaders/` is no longer shipped, being compiled in.
