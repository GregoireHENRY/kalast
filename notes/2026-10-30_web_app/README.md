# 2026-10-30 — kalast in the browser, for kalast.greghy.com

The user (30 October): "ready to setup kalast app on web at kalast.greghy.com to let users use it quickly?", after
asking (29 October, to the edds_decoder session, whose brief came here) for "a page kalast.greghy.com fully public
[...] and have the kalast app available on web and calculation run on the side of users not on my pc". Then, seeing
the first page -- the binary in a canvas under a panel of HTML sliders: "the web app has to be the exact same as the UI
app, which doesnt seem to be the case".

## Progress

- [x] kalast compiles for `wasm32-unknown-unknown`: the `web/` crate, kalast without Python.
- [x] The renderer's bind groups merged to fit WebGPU's four a pipeline; the desktop's frames pixel-identical.
- [x] The page is the UI app, the same code as the desktop's: toolbar, renderer, editor and documentation tabs, the
      side panel's app, simulation and files tabs, the log's three tabs. Opens as the UI app does, on an empty
      scene -- it opened on `examples/didymos/tpm.rs` until the user: "this is not the case in the UI app it should
      not be the case online" -- or on the file its address names (`#examples/sphere/tpm.py`).
- [x] The page's files: the repository's examples and the resources they read, compiled in; what the visitor makes,
      saves or deletes kept in the browser's storage for the next visit.
- [x] Play runs the Rust examples the page was built with: `didymos/tpm.rs` and `crater_self_shadow/fn.rs` run as on
      the desktop; `crater_self_shadow/main.rs`, which drives its own loop, shows its first frame and says why.
- [x] Python scripts open, are edited and saved; sent to the renderer, the log says a browser has no Python.
- [x] Per-facet shadows (`access_shadow_map`) in the browser, a frame or more behind; facet picking on the CPU.
- [x] Cmd on a Mac, and copy and paste through the system clipboard.
- [x] Python in the page: CPython (Pyodide) with kalast's package unchanged and the page's own `kalast._rs`; every
      example of `examples/README.md`'s "work directly" runs but `mesh/decimate.py` (pymeshlab) and `tpm_plot.py`
      (which reads what `tpm.py` saves as it ends). The user, asked whether to do it: "yes do it".
- [x] Deployed at kalast.greghy.com (30 October, after the user's "run this command yourself" and the login's
      Allow): Cloudflare Workers static assets, the WebAssembly 19 MB raw and 7.9 MB over the wire (Brotli); the
      page asked for again each visit, the versioned folder kept a year. Checked live: the default example plays at
      60 fps, `sphere/tpm.py` and `crater_self_shadow/fn.py` run in its Python.
- [x] A colleague's first script in the page, `two_spheres/main.py`, failed at 13:29 UTC with Python's
      `shutil.ReadError: /tmp/... is not a zip file`: their tab was opened before the 13:13 deploy, which took its
      build's folder off the site, and kalast's Python package was fetched from that folder only when Python
      started -- a 404 with no body. It is fetched as the page opens now (see Build and deploy). And an address
      naming a script shows it at iteration 0, as `kalast <file>` does: the page's Python is installed a moment
      after the first frames, and a script asked for before then was said to need the desktop app and dropped.
- [x] The log's clock is local in the page, as in the UI app: it was UTC, `app::clock` asking the operating system
      for the time zone, which a page's WebAssembly has none of; it asks JavaScript's `Date` now. The user: "yes".
- [x] The page runs as fast as it can with `rate_limited` off, as the UI app does. The user, told it ran at 60 frames
      a second: "yes, why is it not the case already? thats the pc app behavior no?" In headless Chrome on this Mac,
      `two_spheres/main.py` went from 60 to about 2,000 iterations a second, `crater_self_shadow/fn.py` 2,400,
      `sphere/tpm.py` 880, `didymos/tpm.rs` 670 (its model time still 0.5 h a second). See "The loop".
- [x] `rate_limited` switched on stopped the page: the cap's wait slept (`State::wait_out`), and `std::thread::sleep`
      panics in a browser. The page leaves a frame until it is due instead.
- [x] The tab's icon is kalast's logo (`res/kalast-256.png`, the desktop window's icon), where it had none.
- [x] The page works in a folder of the visitor's, Chrome and Edge. See "A folder of the visitor's".
  - The user (30 October), for a visitor wanting `examples/landmark_tracking`, whose data goes beside `main.py`:
    "add a button to synchronize these examples folders to a local path chosen by users [...] Also add a button to
    connect local scripts next to examples, maybe you prefer suggesting a different way of doing this".
  - Offered one folder, as the desktop app works in its own, and chosen.
- [x] `Mesh(path=...)`, `Mesh.load`, `mesh.color_modes` and `sim.add_mesh()` in the page's Python.
  - With its data in the folder, `examples/landmark_tracking/main.py` runs to its end in the page: 631 frames in
    75 s, then "saved track.csv: 1893000 rows".
  - Its `track.csv` (258.7 MB) lands beside `main.py`; the one on the data's share is 258.67 MB, the 3,000
    landmarks being drawn at random each run.
- [x] A script's `app.config.width`/`height` sizes the page's canvas, held inside the browser's window (`page_fit`).
  - The canvas is a page's window. Sized as the script asks, the scene, its exported frames and `project_facet`'s
    screen coordinates come out as on the desktop: landmark_tracking's 1020 by 1020 where the browser has the room.
  - Not past the browser's window, though: the 1020 by 1020 left the log panel below the bottom of an 800-pixel
    window, and a page does not scroll. A desktop's window is held to its screen the same way.
- [x] Frames exported in the page, as the user asked: "frame export should work otherwise it is useless".
  - `export_once()` and `export` write `out/frames/000000.png` on into the visitor's folder, numbered after those
    already there, as on the desktop.
  - The page has no threads: each frame's copy is PNG-encoded on the page's thread once it is back (fast
    compression) and written to the folder, not kept in memory.
  - With no folder open, the page says once that its frames go to one.
  - landmark_tracking: 631 frames of 1020 by 1020 beside its `track.csv`, the run 105 s rather than 85.
- [ ] The rest of the Python API in the page: radiance (`tpm.emit`), the GPU's TPM, view factors, shadowing,
      roughness, the GPU readbacks a script asks for (hemicubes, facet ids); figures a script shows (`plt.show()`)
      have nowhere to go yet.
- [ ] An address naming a file of the visitor's folder (`#scripts/mine.py`) opens nothing: the folder is read after
      the page has looked for the file.
- [ ] A smaller download: 17 MB of WebAssembly before Cloudflare's compression; `wasm-opt` not run.
- [ ] Phones: touch, and the editor's layout on a small screen.
- [ ] TIRI's data and simulations for token holders, through tiri.greghy.com's cookie.

## The page is the UI app

`web/src/lib.rs` does what `kalast` does on the desktop: `App::editor_start`, with the file the address names if it
names one, as the command line would, then `App::spawn`, the browser's `start()`. The editor, its panels and the renderer
are kalast's own; nothing of the page is drawn in HTML but a message before kalast starts, or where WebGPU is missing,
or over the canvas if kalast panics.

What the desktop finds on its disk a page has to bring:

- **Files** (`kalast::fs`). Every file kalast reads or writes for the editor, a script or the settings goes through
  one module: `std::fs`'s own functions, re-exported, on the desktop -- nothing changes there -- and in a browser a
  tree in memory. `web/build.rs` compiles in `examples/` as git lists it (less `examples/old/`, and five data files of
  0.2 to 2 MB that only Python examples read) and four files of `res/`: the meshes the Rust examples load and the
  picture the documentation shows. `web/src/lib.rs` keeps what the visitor makes, saves or deletes in
  `localStorage`, put back over the page's files at the next visit; the UI app's remembered settings are a file of
  that tree too (`.kalast/settings.toml`).
- **Rust examples** (`app::cargo::add_example`). A page has no cargo to build an example with, nor `dlopen` to load
  it. `web/build.rs` compiles each `examples/**/*.rs` into the page as a module of its own, and Play runs its `main`
  as the desktop runs a loaded library's: hosted (`app::hosted`), its scene adopted by the page's app. An edited
  example cannot be rebuilt: the log says so.
- **The editor's turn between frames** (`App::page_tick`). On the desktop `editor_tick` runs between frames, outside
  winit's handler; a page's loop is the browser's, so the same requests -- a reset, a mesh, an example, a script --
  are served from `about_to_wait`. A Python script goes to the page's Python (below), waiting for it if the page
  has not installed it yet.

## Python in the page

The user (30 October), asked whether to run Python in the page: "yes do it". 73 of the 75 examples are Python.

CPython compiled to WebAssembly, Pyodide 314.0.7 (Python 3.14, numpy 2.4, scipy, matplotlib), from jsDelivr, started
the first time a script runs or a line is typed at the python tab (`web/python.js`) -- 1.5 s here. On it, kalast's
Python package as the repository has it (`build.sh` zips `kalast/`), and in place of the extension module it wraps on
the desktop, the page's own `kalast._rs` (`web/python/kalast/_rs`): the same modules and names, each object a handle,
by a kind and an index, on the engine already running in the page, each attribute and method a call into
`web/src/python.rs` through JavaScript. Scripts run as the UI app's scripts' thread runs them -- `kalast.editor`'s
`run_toplevel`, `App()` the app on screen, prints to the script tab, the python tab served between frames -- that
code unchanged too.

- **`step()`** waits for the page's next frame: `run_sync` on a promise the page resolves after each frame
  (`PagePython::frame`), which needs JavaScript Promise Integration (Chrome, Edge). Elsewhere a script's `start()`
  returns and its callbacks run with the page's loop; a loop of `step()`s says it needs those browsers.
- **Callbacks** are `Tick::Page`: called with the simulation not borrowed, as the desktop's Python ones are, a
  traceback shown and the callback disconnected when it raises.
- **Runs** are kalast's own: `App::page_tick` takes a script as the desktop's scripts' thread does, its scene reset
  (`begin_script`), and not while another runs -- that one sees the request (`superseded`) at its next `step()` and
  ends, as on the desktop.
- **The config** comes from the same walk as the desktop's bindings: `tools/gen_bindings.py` writes
  `web/src/config_gen.rs` beside `src/py/app/config_gen.rs`, with the same conversions and messages; the three
  hand-written fields (colour mode, sRGB mode, colormap) are written by hand in the bridge too.
- **Arrays.** The engine is another WebAssembly memory, so arrays are copies. The thermophysical model's steps change
  the script's own array: `python.js` hands numpy's memory (`getBuffer`) to the step, which writes back into it.
  `body.mat`, `mesh.colors`, a `Ground`'s arrays -- numpy views onto kalast's memory on the desktop -- write back when
  assigned into (`_Linked`). A `Ground` lives in the engine's memory, so a step copies nothing more.
- **Files.** The page's files are written into Python's own before each run, and `out/` made; a mesh a script writes
  and loads goes into the page's files for the engine. Only `scripts/`, `examples/` and the settings are kept for the
  next visit.
- **Packages.** A package Pyodide has is loaded the first time anything imports it -- scipy from kalast's own
  modules -- where the browser can hold the import (JSPI); else the script's own imports, before it runs. SPICE has
  no build for the page: a stand-in `spiceypy` imports and says so when called.
- **What is not there yet** is a module or a name that says so where it is used, naming the desktop app.

Checked in headless Chrome, a page an example (`#examples/...`), played: `cube/*.py` (light's loop of `step()`s,
per-facet colours written into `mesh.colors`, the gray colormap), `crater_self_shadow/main.py` and `fn.py` (callbacks,
the HUD, `facet_illumination`), `sphere/main.py`, `tpm.py` (its line as on the desktop: "skin depth 1.38 cm, column
8.7 cm in 51 layers, dt 53.6 s, 403 steps a spin"), `tpm_logo.py` (`Orbit`, a graded `Ground`), `tpm_obliquity.py`,
`tpm_variable_1.py` and `_2.py` (a `Ground` changed in place), `two_spheres/main.py`, `misc/cbar.py` (matplotlib,
drawn, not shown); of `analytical/`, `cavity_heating.py`, `sinusoidal.py` and `slab_relaxation.py` to their end.
Restart during a loop of `step()`s ends it and runs again; the python tab prints, completes and sets the config of
the scene on screen.

## What else the browser needed

- **Prints.** kalast's `println!` goes to the kalast tab through a queue (`gui::drain_page_output`): a page has no
  stdout, and an example it runs is the same crate, whose host was `engine_write` again -- an endless recursion, the
  first thing the page hit.
- **No waiting on the GPU.** A page cannot block on a map. The per-facet shadows (`penumbra::Pass::facets_later`)
  hand back the answer an earlier frame asked for -- the facets lit until there is one -- and ask for the next; the
  desktop's `facets` reads as before, its compute split into `facets_submit`. Picking a facet uses the ray on the CPU,
  exact, rather than the GPU's id texel. The other blocking readbacks (hemicubes, horizons, frame export, reference
  images) are not reached from the UI in a browser.
- **Driven examples.** A call inside the page's loop cannot wait for the next frame, which comes after it returns: a
  hosted example's `step()` ends its run there (`hosted::host::step`), the first frame shown and the reason printed.
- **The pointer.** A browser does not let a page move the pointer, nor hold it without a click; the WASD camera's
  `set_cursor_position` and `set_cursor_grab` were unwrapped and stopped the page. Their errors are ignored now,
  which is right on Wayland as well, where moving the pointer is refused too.
- **No language servers, no updates, no quitting.** The editor starts no language server in a page, the update
  check does not run, and Cmd/Ctrl+Q or a script's `close()` leave the page's loop running.
- **egui-winit** (`web/patches/egui-winit`, see its `PATCH.md`): besides the `DroppedFile` fix, a Mac is told from
  the browser's user agent so that Cmd is the command key, and copy, cut and paste reach the system clipboard
  (`navigator.clipboard`; a paste arrives a frame later).

## The loop

The desktop's loop draws a frame each turn and presents one a refresh of the display (`present_interval`): with
`rate_limited` off it runs as fast as it can, the frames between two presents drawn and stepped and not shown. A page
had only the browser's frames, once a refresh (`requestAnimationFrame`, where winit delivers `RedrawRequested`), and a
WebGPU canvas has no immediate mode: one frame a refresh, 60 iterations a second. Now the page's own turns draw frames
too (`App::page_frame`, on winit's poll turns, `scheduler.postTask`), not shown -- the same handler, its present left
out -- and the browser's frame shows the last of them with the panels, beginning none itself while the run goes
(`page_holds`). As on the desktop, a frame waits:

- for the GPU, at most two turns' work queued: counted by `on_submitted_work_done` (`Window::count_in_flight`,
  `gpu_backlogged`), since a page cannot block on `device.poll` as `render_tail` does;
- for its turn under the cap (`State::frame_due`), left for a later turn rather than slept out;
- for a script that has not asked for it (`PagePython::held`): starting, importing, between two `step()`s -- or
  Python's start went by in thousands of iterations. A `step()` returns on a frame drawn, not on a turn that drew
  none.

Paused, the browser's frames are all there is, as before. The panels stay at the display's rate under any cap, where
the desktop's sleep holds them to it. The note that vsync off got Fifo "so the loop runs at the display refresh rate"
is no longer printed in a page, where it is no longer so.

## A folder of the visitor's

A page has no disk, so a script's data had no way in: `examples/landmark_tracking` wants three files from a cloud
link beside its `main.py` (4 MB; the share's `frames.zip` and `track.csv` are what it writes). The share sends no
CORS header, so the page cannot fetch them itself either.

**Opening a folder.** *Open a folder of yours...* in the files tab (`gui::page_folder_row`) asks the browser for a
folder (`showDirectoryPicker`, the File System Access API: Chrome and Edge, the browsers that already run a loop of
`step()`s). The page then works in it, as the desktop app works in its own:

- It puts there the page's `examples/` and an empty `scripts/`: those missing, nothing replaced.
- Its files go over the page's own (`folder_put`, `kalast::fs::add`, quietly).
- What the page changes is written there instead of the browser's storage (`folder::takes`, from `lib.rs`'s
  listener): the editor's saves, new files, renames, and what scripts write. The settings stay the page's.

**Edited with the visitor's own tools.**

- The folder is read again when the page gets focus or becomes visible, and before each run (`folder.js`'s
  `refresh`).
- An open file without edits of the editor's own is read again (`Editor::changed_outside`), so that Play runs what
  the visitor wrote.
- Not read: hidden folders, `__pycache__`, `node_modules`, `target`, and files past 256 MB.
- `out/` is listed but not read: exported frames and results by the hundred, which would otherwise be held in memory
  at every visit. A script that reads back what an earlier visit wrote there will not find it.

**Python.**

- Python's files now take only what changed since the last run (`py_changes`); before, every file was copied before
  each run.
- What a script writes goes back after the run, and every 2 s while it runs, since a loop of `step()`s may never
  end (`python.js`'s `outputs`).
- It goes through the page's files, which show it. Past 64 MB it goes to the folder alone.

**Remembered.**

- The folder is kept in IndexedDB. The next visit opens it again in one click, or at once where the browser keeps
  the permission.
- The cross beside its name forgets it and reloads the page on its own files.

**Nothing in it is deleted from the page.** A browser has no Trash, so the files tab's Delete says the file stays
and to delete it there. A rename's old name is taken away only once the new one is written.

Checked in headless Chrome, the picker stood in for by a folder of the page's own storage (OPFS, the same handles):

- picked, it got the 96 files of `examples/`;
- after a reload it was opened again without asking;
- landmark_tracking ran on the data put beside it;
- a script made and then changed outside the page was listed, shown with the change, and printed it when run;
- Delete said the file stays, and it stayed.

Without a folder, six examples played as before.

## Checks

In headless Chrome with WebGPU (playwright-core, scratch scripts): the page opens on the example held at iteration 0;
Play runs it at 60 frames a second; the files tab lists `examples` and `scripts`; a new file made in `scripts`,
edited and saved with the toolbar's button, is there after a reload (`localStorage` holds it); the editor's and the
documentation's tabs draw (the README's screenshot too); `crater_self_shadow/fn.rs` runs with its shadows and its
"lit %" (44 % at iteration 306); a click on a facet picks it; copy and paste go through the system clipboard with Cmd
on a Mac user agent. At twice the pixel density, with Chrome's scale forced and emulated both (either alone reports
sizes a real screen never does), the panels are sharp and laid out as at once.

The page's colours: a WebGPU canvas offers no sRGB format, and drawn in its plain one the image came out darker than
the desktop's (inferno's pale yellow end a saturated yellow-green); kalast now draws through an sRGB view of the canvas
(`Window::format`), the colours as the desktop's sRGB surface encodes them. The desktop keeps its surface's format.

On the desktop: the release binary builds without a warning; the library's 401 tests pass (release); the stubs, the
config panel and the crate's package check pass; `examples/didymos/tpm.rs` precompiled as the release workflow does
(`kalast --precompile`) and run in a window of its own; the throwaway program of the morning drew its five frames --
the data mode with the colour bar, shadow maps, the Sun's disc, rays, photometry -- 0 pixels apart from the morning's.

`browser.png`: the page in Chrome (headless, Retina density, WebGPU on the M-series GPU), the example 3 h in.
`python.png`: `examples/sphere/tpm.py` in the page's Python, its line in the script tab.

## Build and deploy

From `web/`:

    rustup target add wasm32-unknown-unknown
    cargo install wasm-bindgen-cli --version 0.2.128
    ./build.sh
    npx wrangler deploy        # after `npx wrangler login`, once

`dist/` is the whole site: the page, and its script and WebAssembly in a folder named for their content (the
greghy.com zone keeps scripts 4 h in browsers whatever the server says). `wrangler.jsonc` serves it as a Worker's
static assets at kalast.greghy.com, on Cloudflare, up with this Mac off. `_headers`' rule for that folder is
`/:version/*`: a pattern holds one `*` only, and the first, `/*/kalast_web*`, was dropped without a word -- the
assets went out with `max-age=0`.

A deploy replaces the site's files at once: the previous build's folder is gone, and a page opened on it can fetch
nothing more from there. So a page fetches all of its own files as it opens -- the WebAssembly, the scripts, and
since 30 October kalast's Python package (78 KB), which `python.js` fetched when Python started: a colleague's tab
opened before the 13:13 deploy asked for it at 13:29 and got a 404 with no body, which Pyodide unpacked into "is not
a zip file". Pyodide comes from jsDelivr, its version in the address, and stays there.

Shape models: DART's Didymos v003 (Palmer et al.) and Dimorphos v004 (Daly, Ernst et al.), SPC, decimated to 10,000
facets, in `res/`.
