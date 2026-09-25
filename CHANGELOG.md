# Changelog

Each version's section is the text of its GitHub release, verbatim and
nothing else, and it is written for the people who *use* kalast: what
changed for them. Release engineering, CI and internal refactors do not
belong here. The version gate refuses a tag whose section is missing or has
no entries, and the section is approved by a person before the tag is
pushed. While betas of a version go out, its section is headed
`## v<version>-beta`; it becomes `## v<version>` when the version is
released, and the gate refuses the tag until it has.

## v0.5.10-beta

- The UI app's log panel has two tabs: **kalast**, shown first, with what kalast says about itself -- loading, the update check, builds -- and **script**, with what the script prints -- `print`, tracebacks, `app.log` -- so the first no longer lands in the middle of the second. Everything still goes on to the terminal.
- A plane view from the navigation gizmo -- or any orthographic camera -- holds still while the bodies move, as the perspective view does: on the Didymos pair it panned and zoomed with Dimorphos's orbit. Switching between perspective and orthographic keeps the size of what is at the camera's anchor.
- The log's kalast tab says when the simulation pauses and when it runs again, and at which iteration, whether by `P`, the Play, Pause and Step buttons, `pause_after_iteration` or the script.
- `app.simulation.state.pause_at` is now `pause_after_iteration`, one less: `pause_after_iteration = 0` pauses once iteration 0 has run, as the log says. `pause_at` still works in this version, with a deprecation warning.
- The deprecation warnings for old names -- `load_mesh(flatten=...)` and the flat config names such as `config.debug_window` -- now show, on the script's line. Python hid them by default, so a script using an old name was never told.
- A log tab you are not looking at shows a small dot when new lines come into it.
- The log panel keeps its height when switching tabs, and can be dragged taller than the text in it.
- The UI app is laid out like VS Code, its panels in rounded cards. The middle shows the **renderer** or the **editor** (the script), chosen with two buttons at the start of the toolbar, so folding the toolbar with `↑` leaves the scene alone; Play or Restart switch back to the renderer. A panel's edge lights up when it can be dragged. The side panel on the right has three: **app** (the app's settings), **simulation** and **files**, the folder kalast was started in as a tree, which is where a script or a mesh is opened now, with a click -- over unsaved edits it asks first. The toolbar names the open file, with **save** beside it, and the editor fills the middle. The script panel on the left is gone, with its open button and path field, and with it what `←` and `app.config.script_folded` did.
- The log has a **python** tab: a Python console whose lines run between frames among the running script's variables, `app` included, so a paused scene can be inspected and changed -- while a script runs its own loop too. `Tab` completes names and attributes, and `↑` and `↓` recall earlier lines.
- `help(body)` explains a body's `mat` and `mesh`.
- Printing `app.simulation` or a mesh shows a one-line summary -- bodies, facets, iteration -- instead of every vertex, which froze the UI app on a full-resolution model.
- `Cmd` + `Q` quits the UI app whatever it is doing -- it did nothing while a script was loaded -- and asks first over an edited script.
- Running another script, or opening a mesh, in the UI app starts from a new renderer. The last script's meshes were still drawn for the new bodies when there were as many, its `before_render`/`after_render` kept running, and its config, camera, selection and export carried over. Playing the same script again keeps the config, so changes made in the panel stay.
- The UI app's panels are drawn in Catppuccin Mocha; `app.config.theme = "dark"` gives egui's dark theme instead. The scene and its background are not affected.
- The theme and fullscreen are remembered between sessions when changed in the UI app -- the app tab, `F`, the green button -- in `settings.toml` in your user configuration folder. A script that sets them changes nothing remembered.
- Clicking a facet of a finely resolved shape model in kilometres -- Dimorphos at full resolution -- selects it: the click went through to Didymos behind. The same fix applies to `kalast.mesh.intersect_mesh` and `pick_facet`.
- Each line in the log panel shows the time it was printed, to the millisecond, and each tab opens with a line saying when the UI app started, with kalast's version in the kalast tab.
- What a script prints reaches the log from its first line, a script named on the command line included: those ran before the log was listening, and their `print` only reached the terminal. Printing a lot before the first frame no longer hangs the UI app.
- `examples/crater_self_shadow/main.py` and `main.rs` drive their own loop, as `step.py` and `step.rs` did; the callback versions are now `fn.py` and `fn.rs`.
- `examples/hera_didymos/afc.py` and `afc_eclip_didy.py` pause at the end of their date range instead of starting over, and `afc.py` no longer writes image positions: `examples/landmark_tracking/main.py` is the example for those.
- `examples/README.md` describes every example, and the README links it beside the Python API, config and controls references.
- Betas: between releases, the next version's bundles are published as the pre-release `v<version>-beta`, each replacing the last. A beta bundle's update button offers the newer beta, and then the release; nothing else is offered a beta.

## v0.5.9

- Double-clicking `kalast` works: started with nothing to open from outside its folder -- which is how the macOS Finder starts it, from the home folder -- it moves into its own folder first, so the examples find `res/` and their pre-compiled Rust libraries. Before, every example stopped on its first mesh.
- The Linux bundle runs on Ubuntu 22.04: v0.5.8 was built against glibc 2.39 and refused to start there with `GLIBC_2.39 not found`. It needs glibc 2.35 or newer now.
- The Linux wheel on PyPI is built for glibc 2.35 or newer, like the bundle, instead of 2.17: it uses the newer versions of glibc's `expf`, `logf`, `powf`, `exp`, `log`, `pow` and `hypot`, not their compatibility versions. On an older system `pip install kalast` builds from source, which needs Rust.
- The README says how to run a bundle downloaded on a Mac -- `xattr -cr` on the unpacked folder, once -- and which Linux systems the bundle runs on, and what else it needs there.

## v0.5.8

- `app.simulation.project(point)`, `project_body(body)` and `project_facet(body, facet)` give where a point, a body's centre or a facet's centre lands in the image: `(x, y)` in pixels from its top-left corner, x right and y down, up to `app.simulation.image_size`. `examples/hera_didymos/afc.py` writes the bodies' and the selected facets' positions to `out/hera_didymos/afc/screen.csv` at every frame.
- `examples/landmark_tracking/main.py` follows 3000 facets of a Dimorphos shape model through a sequence of camera and Sun positions: it exports every frame and writes each facet's position, its pixel in the image and the cosine of its viewing angle to `track.csv`. Its input data is linked from the folder's README.

## v0.5.7

- The toolbar folds with the other panels: `N` folds all four, its lower edge can be dragged up or double-clicked like theirs, and the handle it leaves brings it back. `app.config.panels_folded` now means all four.
- Arrow keys fold or unfold one panel each, the one on the edge the arrow points to: `↑` toolbar, `↓` log, `←` script, `→` simulation. From a script: `app.config.toolbar_folded`, `log_folded`, `script_folded`, `simulation_folded`, live, and reading back what a key or a drag did.
- `F` (and the green button) on an external monitor: the window went fullscreen and came straight back out. It stays fullscreen now.
- Focus mode's toolbar, summoned at the top edge, is one row tall like the docked toolbar instead of a fixed height with an empty band under the buttons.
- The UI app's `open` button takes a `.obj` as well as a script: the mesh is shown the way `kalast some.obj` shows it -- the scene emptied, the camera and Sun placed from it, Blender axes and wireframe on.
- Windows: a pre-compiled `.rs` example that panicked while loading -- a mesh path it could not find, say -- took the UI app down with it. The panic is caught at the example's edge now and reported in the log panel, and `kalast.exe` runs with a 16 MB stack instead of Windows' 1 MB, since a hosted example's `main` drives a frame from inside a frame.

## v0.5.6

- The UI app checks for a newer release when it opens -- on a thread, never in a script's run, off with `app.config.check_updates = False` -- and if there is one, the log shows this version and its release date, the new version and its, and its notes, and the toolbar gets an **update** button that installs it in place (a bundle folder by folder, a pip install with `pip install --upgrade`) and then a **restart** button. `kalast --update` does the same from a terminal.
- The cube examples draw their wireframe two pixels wide, `cube/light.py` turns its Sun ten times slower so it reads at the frame rates the window now reaches, and `cube/color_map.py` names its colormap.

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
