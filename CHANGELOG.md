# Changelog

Each version's section is the text of its GitHub release, verbatim and
nothing else, and it is written for the people who *use* kalast: what
changed for them. Release engineering, CI and internal refactors do not
belong here. The version gate refuses a tag whose section is missing or has
no entries, and the section is approved by a person before the tag is
pushed. While betas of a version go out, its section is headed
`## v<version>-beta`; it becomes `## v<version>` when the version is
released, and the gate refuses the tag until it has.

## v0.5.14-beta

- `app.simulation.meridian_facets(body)`: the facets along a meridian, pole to pole, and the latitude each has, on the body's shape as its pose stretches it.
- `examples/sphere/tpm_plot.py` draws a sphere TPM run: the surface at each latitude over the last two spins, and the equator's and the poles' columns spin after spin, with whether they have converged, and through the last spin, the daily wave going down -- for `tpm_logo.py`, with its orbit, the last two years and the last year beside them, the seasons. Every sphere TPM script saves what it draws when its run stops.
- A graded column (`core.Ground.graded`) settles all the way down. Its thick deep layers stepped by less than float32 holds near where they settle, every step was rounded away, and they stopped short: the logo's 11 m columns by up to 10 K, which left its surface up to 1 K too warm near the poles. A `Ground` now carries what a step leaves over into the next one; columns of equal layers are unchanged.
- `app.simulation.config.image.width` and `height` pin the image in the editor too, rendered at that size and fitted into the viewport, and in a plain window from its first frame: a size set before the window opened was ignored, and the export came out at the window's physical size -- twice its size on a Retina screen.
- `app.simulation.config.image.flip_x` and `flip_y` mirror the image, left to right and top to bottom, on screen and in exported frames, to match an instrument whose images are stored mirrored: any corner of the view can be pixel `(0, 0)`. The HUD and the colour bar are drawn as they are; picking, the axes, the navigation gizmo and the mouse follow the mirror.
- `examples/sphere/tpm_logo.py` starts each column at the temperature its mean sunlight over an orbit gives it rather than 0 K, and runs 30 Didymos years rather than 3: its temperatures have converged down to the bottom of its columns, where after 3 years from 0 K they were still rising.
- `examples/hera_mars_swingby/tiri_diffuse_light.py` simulates TIRI's images of the Mars swing-by on 2025-03-12, as `hera_didymos/afc.py` does AFC's: Mars from a DTM with its relief exaggerated ten times, Deimos and Phobos, at TIRI's 1018 x 768 and mirrored top to bottom to match its images, exported a minute apart from 05:52 to 16:00 UTC and every 5 s from 12:07 to 12:10, Deimos's closest pass. It replaces `diffuse_lighting_one_image.py`.

## v0.5.13

- A downloaded bundle has `examples/` at its root again, beside an empty `scripts/` for your own scripts. An update never touches `scripts/`, and before it replaces `examples/`, moves every example you changed or added there, whole, to `scripts/backup/before-v<version>/`.
- The wireframe is on by default, drawn over the shaded mesh (`wireframe.mode = 2`), in very dark grey `(0.01, 0.01, 0.01, 1.0)` rather than black; the examples no longer set that colour themselves.
- `K` steps a script that runs its own loop -- `while app.running:`, as `examples/sphere/main.py` does -- as the Step button does. It moved the iteration counter and nothing else.
- In `examples/sphere/tpm_logo.py` the time step divides Didymos's spin exactly, so drawing a frame a spin (`steps_per_frame = steps_per_spin`) shows it at the same turn every frame instead of creeping round.

## v0.5.12

- The bundle's script editor has a Python language server with nothing to install: ty, Astral's, ships in the bundle -- completion, hover, signatures, errors and go to definition, on kalast's own API too. For `python -m kalast`, `pip install "kalast[editor]"` brings it. basedpyright, which catches more type errors, is one setting away: `app.config.python_language_server = "basedpyright-langserver --stdio"`.
- The editor's completion list keeps a name and its type apart: with ty, `step` read as `stepbound method App.step() -> bool`.
- Saving a script that has no file yet -- typed into the empty editor -- asks where to save it. In Neovim, `:w` said `E32: No file name`.
- Neovim in the editor starts with a config kalast ships: its author's settings, keymaps and editing plugins (surround, autopairs, git hunks), installed the first time with git, apart from your own Neovim's. `app.config.neovim_config = "user"` keeps your own config, or a path points to any other.
- Errors in the bundle's scripts are reported wherever the bundle is unpacked. Inside another project -- its `dist/`, or a folder git ignores -- ty counted them out of that project and said nothing; the bundle's `ty.toml` now makes it a project of its own.
- In Neovim, a space or Enter stays where it is typed. The cursor jumped to the line under the mouse pointer, and two quick spaces selected a word, the letters after them then taken as commands.
- On macOS, Neovim types what Option types -- `{`, `[`, `|`, and `~` or accents with a dead key -- where it left insert mode. Cmd+C and Cmd+X copy and cut the selection, and Cmd and Option with the arrows and Backspace go by line and by word, as in VS Code.
- On Windows and Linux, Ctrl+V in Neovim begins a Visual block again after insert mode; it pasted.
- In Neovim, `/` and `?` highlight their matches as in a terminal: all of them while `hlsearch` is on, the current one apart, as the pattern is typed, until `:noh`.
- The UI app opens where it was closed: on the same screen, at the same place and size, maximised or fullscreen if it was. Unticking **remember last window** in the app tab fixes where it opens instead -- screen, place, size, fullscreen -- and `app.config.monitor`, `window_x`, `window_y`, `width` and `height` set them from a script.
- The side panel's settings show their names whole where there is room, and a text field stays within the panel.
- The files tab is the **scripts** tab: two folders, `examples` and `scripts`, your own. A right click adds an example, a folder or a file, renames either, or moves either to the Trash; a file also goes to the renderer, and Markdown to the documentation tab. The button beside the tabs opens any text file from anywhere.
- Updating the bundle, from v0.5.10 too, keeps the examples you changed or added: each such example moves whole to `scripts/examples-before-v<version>`, where it still runs, and the kalast tab says which; the others are replaced by the new version's. `scripts` is never touched.
- The editor opens any text file, and keeps every file opened, with its edits, in a list on its left, under which is the shown file's outline -- classes, functions, variables; a Rust file's items; Markdown's headings -- a click going to the line.
- Opening a file no longer changes the scene: the editor's **render** button sends the file shown to the renderer, from a clean scene each time -- a script, a Rust example, a mesh, which is shown held -- and Play, Restart and Step act on what was sent. A Rust example's **compile**, debug or release, is beside it.
- In the script editor, `Cmd`+`Enter` -- `Ctrl`+`Enter` on Windows and Linux -- sends the file shown to the renderer, as the **render** button does; in Neovim too.
- A Markdown file opened as documentation is a page of the documentation tab until kalast closes, and a link from a page to Markdown on this disk opens there too.
- Reset puts the toolbar's iteration back to 0; it went on showing the last run's.
- A folder named `mesh` is a plain folder in the scripts tab, not a database.
- The bundle ships pandas: `examples/landmark_tracking/main.py` stopped on `No module named 'pandas'`.
- In Neovim, an error such as `E492: Not an editor command` goes as Insert mode, Visual mode or a new command begins, as in a terminal, 'showmode' on or off; it stayed on screen, and came back after the next command.
- In a bundle, the script editor's language server and Neovim start as they should. They inherited the bundle's Python settings, so a Python-based one -- such as the basedpyright Neovim's mason installs -- exited at once.
- New examples in `examples/sphere`: a spinning sphere's thermophysical model, with thermal properties varying over the surface and with depth, with obliquity, and kalast's logo simulated again through Didymos's seasons.
- `kalast.tpm.core` runs a thermophysical model over every facet of a body at once, one call per physical step: `columns` makes the temperatures, a column of layers under each facet; `solar_bc` balances each surface's absorbed sunlight against what it radiates and conducts down; `bottom_adiabatic` closes the columns; `heat_conduction` conducts through them. The scheme, named in their documentation: explicit finite differences at a fixed step, forward in time and centred in depth, the surface solved by Newton's method.
- `kalast.tpm.core.Ground` gives a body's thermal properties facet by facet and layer by layer -- albedo and emissivity per facet, conductivity, density and heat capacity per layer and facet -- as numpy arrays changed in place, and `solar_bc` and `heat_conduction` take it where they take `Properties`. The conduction keeps the heat flow continuous across a change of material.
- Each column of the thermophysical model can start at its own latitude's effective temperature, near where it settles, in one line: `core.columns(nz, nf, core.effective_temperature(dau, sim.facet_mean_incidence(0), prop.albedo, prop.emissivity))`. `app.simulation.facet_mean_incidence(body)` gives the sunlight each facet gets on average over a spin, from the body's pose and the Sun -- none in the polar night, all spin long in the polar day -- and `kalast.tpm.core.mean_incidence(lat, dec)` the same for a latitude; `effective_temperature` and `columns` take one value a facet. On a tilted sphere every column settles within 18 spins, where one start for the whole body left its polar night 96 K too warm after 50.
- `kalast.astro.Orbit(a, e)`: a Keplerian orbit about the Sun, its `period` and the body's `position(t)` -- the Sun's distance and direction over a year, for seasons.
- `kalast.tpm.core.Ground.graded(prop, facets, dz, depth)` makes a column thin at the surface and thicker with depth, for a day's wave and a year's in one: 36 layers from 8 mm to 20 m, where equal layers would take 2,500 -- a body spun through its seasons in minutes.
- `app.simulation.facet_incidence(body)` gives each facet's cosine of incidence from the body's pose and the Sun's position as set, with no frame drawn and no shadow map. A pose that scales the body -- flattened along its pole, `bod.mat[:3, :3] = turn @ numpy.diag([1.0, 1.0, 0.7])` -- is taken as the renderer draws it, so a shape can stay in `mat` with the mesh as loaded.
- A script that sets the length of its run, `state.pause_after_iteration`, is held at iteration 0 when opened, like any other; it played straight through. Step and `K` keep that length; they replaced it.
- The script editor completes and documents `kalast.tpm.core`'s functions.
- The script editor no longer marks as errors a colormap given by name, `config.data.colormap = "inferno"`, `mesh.values` set from a list, or `body.mesh` used without a check for `None`: every body loaded from a script has a mesh.
- In the script editor, line numbers, the band on the cursor's line, clicks and hovers keep to their line however far down a file. On a Retina screen Neovim's numbers drifted down, a line off by line 79, a click there went to the line above, and hovering a name far enough down showed nothing, in either editor.
- All of matplotlib's colormaps are built in, by matplotlib's names -- matplotlib itself not needed -- where there were four.
- The colormap is chosen in the simulation tab, under Data colouring: a list of the built-ins, each with its colours, a strip of the colours in use, **reverse**, and **load file...** for a text file of colours, a line each, red, green and blue in 0..1 or 0..255. From a script or the Python console, `config.data.colormap` takes a built-in's reverse as `"inferno_r"`, as matplotlib names it, and a path to such a file.
- The colour bar no longer disappears when its settings are opened in the simulation tab. Opening a section of settings never changes them: a slider held a script's value to its own range as it was drawn -- the bar's length to 1 pixel -- and a colour button rewrote colours by a hair.
- The colour bar has an outline (`colorbar.border`, on by default) and a tick mark beside each of its numbers, and `colorbar.min_max` marks and writes the lowest and highest value the bodies carry, on the side away from the numbers, as `colorbar.min_max_format` says -- whole numbers unless set, or `".3f"`, `".2e"`. A vertical bar's caption reads upwards beside it, and the panel's `vertical` is one choice of auto, yes or no.
- The colour bar resizes by dragging its edges in the render window -- an end for its length, a side for its thickness -- and `colorbar.tick_size` sets the length of its tick marks, which sit across the bar's edge, half inside and half out. Its default size is 800 by 36 pixels, from 320 by 18.
- Clicking a navigation gizmo ball again gives the same view, and so do `frame_all` and `camera.view_along`, however the bodies have spun since: the zoom followed a spinning body's bounding box.
- A camera's `dir`, `up` or `up_world` set from a script a hair off unit length -- `[-0.342, -0.651, 0.678]`, three decimals typed -- aborted kalast with a panic; any non-zero vector is now taken as its direction, and a zero one raises `ValueError` where it is set.
- A facet selected in the scene shows its value, the colour the colormap gives that value, and its own colour -- red, green and blue from 0 to 1 -- in the Selection panel, live as a script changes the values, and in the lines the click prints.

## v0.5.10

- macOS: the bundle has a `kalast.app`, with kalast's icon: double-clicking it opens the UI app without Terminal. `./kalast` still runs it from a terminal.
- macOS: the UI app and the Rust examples built from source with Xcode 26 run at full speed. They were held to the display's refresh rate, about 120 frames a second.
- The UI app's log panel has two tabs: **kalast**, shown first, with what kalast says about itself -- loading, the update check, builds -- and **script**, with what the script prints -- `print`, tracebacks, `app.log` -- so the first no longer lands in the middle of the second. Everything still goes on to the terminal.
- A plane view from the navigation gizmo -- or any orthographic camera -- holds still while the bodies move, as the perspective view does: on the Didymos pair it panned and zoomed with Dimorphos's orbit. Switching between perspective and orthographic keeps the size of what is at the camera's anchor.
- The log's kalast tab says when the simulation pauses and when it runs again, and at which iteration, whether by `P`, the Play, Pause and Step buttons, `pause_after_iteration` or the script.
- `app.simulation.state.pause_at` is now `pause_after_iteration`, one less: `pause_after_iteration = 0` pauses once iteration 0 has run, as the log says. `pause_at` still works in this version, with a deprecation warning.
- The deprecation warnings for old names -- `load_mesh(flatten=...)` and the flat config names such as `config.debug_window` -- now show, on the script's line. Python hid them by default, so a script using an old name was never told.
- A log tab you are not looking at shows a small dot when new lines come into it.
- The log panel keeps its height when switching tabs, and can be dragged taller than the text in it.
- The UI app is laid out like VS Code, its panels in rounded cards. The middle shows the **renderer** or the **editor** (the script), chosen with two buttons at the start of the toolbar, so folding the toolbar with `↑` leaves the scene alone; Play or Restart switch back to the renderer. A panel's edge lights up when it can be dragged. The side panel on the right has three: **app** (the app's settings), **simulation** and **files**, the folder kalast was started in as a tree, which is where a script or a mesh is opened now, with a click -- over unsaved edits it asks first. The toolbar names the open file, with **save** beside it, and the editor fills the middle. The script panel on the left is gone, with its open button and path field, and with it what `←` and `app.config.script_folded` did.
- The log has a **python** tab: a Python console whose lines run between frames among the running script's variables, `app` included, so a paused scene can be inspected and changed -- while a script or a Rust example runs its own loop too. `Tab` completes names and attributes, and `↑` and `↓` recall earlier lines.
- In the UI app, a Python script run after a Rust example shows its scene: the viewport stayed empty.
- `help(body)` explains a body's `mat` and `mesh`.
- Printing `app.simulation` or a mesh shows a one-line summary -- bodies, facets, iteration -- instead of every vertex, which froze the UI app on a full-resolution model.
- `Cmd` + `Q` quits the UI app whatever it is doing -- it did nothing while a script was loaded -- and asks first over an edited script.
- Running another script, or opening a mesh, in the UI app starts from a new renderer. The last script's meshes were still drawn for the new bodies when there were as many, its `before_render`/`after_render` kept running, and its config, camera, selection and export carried over. Playing the same script again keeps the config, so changes made in the panel stay.
- The UI app's panels are drawn in Catppuccin Mocha; `app.config.theme = "dark"` gives egui's dark theme instead. The scene and its background are not affected.
- The theme and fullscreen are remembered between sessions when changed in the UI app -- the app tab, `F`, the green button -- in `settings.toml` in your user configuration folder. A script that sets them changes nothing remembered.
- Clicking a facet of a finely resolved shape model in kilometres -- Dimorphos at full resolution -- selects it: the click went through to Didymos behind. The same fix applies to `kalast.mesh.intersect_mesh` and `pick_facet`.
- Each line in the log panel shows the time it was printed, to the millisecond, and the kalast tab opens with a line saying when the UI app started and kalast's version.
- What a script prints reaches the log from its first line, a script named on the command line included: those ran before the log was listening, and their `print` only reached the terminal. Printing a lot before the first frame no longer hangs the UI app.
- `examples/crater_self_shadow/main.py` and `main.rs` drive their own loop, as `step.py` and `step.rs` did; the callback versions are now `fn.py` and `fn.rs`.
- `examples/hera_didymos/afc.py` and `afc_eclip_didy.py` pause at the end of their date range instead of starting over, and `afc.py` no longer writes image positions: `examples/landmark_tracking/main.py` is the example for those.
- `examples/README.md` describes every example, and the README links it beside the Python API, config and controls references.
- Betas: between releases, the next version's bundles are published as the pre-release `v<version>-beta`, each replacing the last. A beta bundle's update button offers the newer beta, and then the release; nothing else is offered a beta.
- On Windows, the log's kalast tab shows what kalast prints -- the meshes it loads, builds, debug output -- as it does on macOS and Linux, `kalast.exe` double-clicked included. It showed only the update check and the pauses.
- On Windows, a script that closed its standard output and then opened a file could find kalast's own lines written into that file. What kalast prints, a Rust example's included, now goes straight into the log.
- Each run of a script -- Play, Restart, a script opened -- starts the script tab with `script log started`, so one run's output reads apart from the last.
- Opening and saving a file in the UI app is logged in the kalast tab, not the script tab.
- The navigation gizmo shows by default (`app.simulation.config.axes.style = "gizmo"`; it was `"off"`), and smaller: `axes.gizmo_size` is 30 instead of 60. It goes into exported frames, so the examples that export -- the Hera scripts, `landmark_tracking` -- set `axes.style = "off"`.
- `app.simulation.config.export.axes = False` keeps the axes -- grid, box or panes, gizmo -- out of exported frames while the window still shows them. Their tick labels now go into exported frames with them, rather than only with `export.hud`.
- The HUD goes into exported frames by default, as it shows on screen; `app.simulation.config.export.hud = False` keeps a data product to the render alone.
- The toolbar's Play, Pause, Restart and Step are VS Code's icons, and a new **Reset** beside them clears the scene as if nothing had been loaded -- bodies, settings, the running script -- keeping the script in the editor for Play. It can always be pressed, an empty scene included, and brings back a new app's camera and the welcome, the simulation paused until Play. An available update shows as a filled button.
- The files tab looks like VS Code's explorer with Catppuccin's icons: each file and folder its icon, a row lit under the pointer and for the open file, chevrons and indent guides; a click anywhere on a folder's row opens it.
- The editor colours Python and Rust in Catppuccin Mocha, numbers its lines and lights the current one. `Tab` and `Shift`+`Tab` indent with spaces, and `Enter` keeps the indentation, one level more after `:` or `{`.
- The python tab reads like Python in a terminal: Python's banner as soon as the app opens, the prompt right after the last line printed, prompts and code in colour, tracebacks in red. `Ctrl`+`L` clears it and `Ctrl`+`C` drops the line being typed.
- The renderer and editor tabs, the side panel's and the log's are drawn as VS Code's, the chosen one underlined.
- Folding or opening a panel in the UI app no longer zooms the scene: the toolbar and the log cover or uncover it, as the side panel always did, and it keeps its size while a panel slides instead of pumping in and out. The camera's field of view now spans the window's height, so with the toolbar and log open the view is closer than before; with the panels folded it is unchanged.
- The editor completes as you type, shows a symbol's type and documentation on hover and a call's signature above it, and underlines errors and warnings with their message at the end of the line, as VS Code does -- from a language server: basedpyright or pyright for Python (`uv tool install basedpyright`), rust-analyzer for Rust; in a release bundle, the bundle's own packages are found. `F12` or `Ctrl`+click goes to a definition, `F8` to the next problem, and the bar under the text counts them. `app.config.language_servers = False` turns this off.
- `app.config.neovim = True` makes the editor your own Neovim, with your config -- modes, `:` commands, searches, macros, your mappings -- as VS Code's Neovim extension does. `:w` saves, `:q` goes back to the renderer, and `K`, `gd` and `]d` ask the language server.
- The editor draws a line at column 80, `app.config.ruler`.
- A mesh opened on its own -- `kalast some.obj`, a click in the files tab -- shows the navigation gizmo like every other scene, instead of Blender's ground grid.
- The editor's settings are remembered between sessions, with the theme.
- The side panel's tabs are icons, and the app and simulation tabs are sections, each with its icon, closed until clicked; every setting is a row with its name on the left and its control on the right.
- The toolbar's Play, Restart, Step and Reset are at its right end, with the iteration and frame rate before them.
- kalast's logo is the icon of its window, of its taskbar button and of `kalast.exe`, starts the toolbar -- its version on hover -- and shows in the empty scene with the keys to start with.
- The Python stubs describe kalast as it runs, so VS Code and the editor stop marking correct scripts as errors: optional arguments are optional (`load_mesh(path=...)`), array settings accept lists and tuples (`camera.pos = [...]`), `sim.bodies[0].mat` is known, `before_render` and `after_render` are called with the simulation, meshes' `vertices` and `facets` have a length and an index, and the modules' functions and constants -- `kalast.entity.DIDYMOS`, `kalast.tpm.properties.skin_depth_1` -- are declared.
- A **documentation** tab beside renderer and editor shows kalast's documentation in the window -- the README, the Python API, the config and controls references, this changelog, and the READMEs of `res/` and `examples/` -- as they were when kalast was built, with an outline of the page. A link to a script opens it in the editor, and a link to a folder shows it in the files tab.
- The Python API, config and controls references are in `docs/` instead of `notes/`.
- The welcome in the empty scene goes at the first orbit, pan or zoom -- a middle-drag, the wheel, the gizmo -- and comes back with Reset. It points at the files tab by the tab's icon.
- A new app's camera looks at the origin from `(8.14, -5.33, 5.67)`, a little above and to one side, instead of from the origin itself, where it could not be turned: the empty scene orbits, and a script that never places the camera sees its bodies from outside. A script that places it with `pos` and `look_anchor()` still gets a level view.
- Opening a Rust example in the UI app loads it; its source was also handed to Python, which stopped with a SyntaxError.
- With `app.config.neovim`, `:w` no longer fails with `E32: No file name` after switching buffers, `:e` opens a file in kalast, and the bar under the text says when Neovim has left the script for a buffer the editor does not show.
- Scripts no longer leave an empty `out/frames` folder where they are run: it is made when the first frame is exported.
- Arrows and other symbols -- `→`, `⌥`, `●` -- show in the UI app instead of boxes.
- The navigation gizmo's balls work in an empty scene: the view turns to look along the axis, about the point it looks at. They did nothing until something was loaded.

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
