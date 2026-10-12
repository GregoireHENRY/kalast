# One shadow method, colour modes by name, the console's own names

12 October 2026, M1 Pro.

The user, on the settings after the ray tracing came over: "i dont understand
what reference button does"; "instead of a toggle ray and cascades toggle, you
should present a choice between shadow map, CSM, and rays, and no shadow";
"no shadow would remove the option color mode 3"; "the shadow option should
appears only when color mode is 0"; "color_mode is now either 0 or 1 and could
be renamed". And a console that lost its variables: `a = mesh`, then
`a.facets` was `'float' object has no attribute 'facets'`.

## `shadows.method`

`shadows.rays` (a switch) and `shadows.cascades` (0 for none, else a count)
were two ways of saying which method draws the shadows, and `color_mode = 3`
a third. One field now, `ShadowMethod` in `config.rs`:

| `shadows.method` | |
|---|---|
| `"maps"` | a layer per body, the default |
| `"cascades"` | cascaded maps, `shadows.cascades` of them (1-4, 3 by default) |
| `"rays"` | traced, on a GPU with ray queries; the maps answer without |
| `"none"` | no shadows |

Neither `rays` nor `cascades` had been released (the beta was cut on 6
October, before both), so they changed without a shim. `Shadows::cascades_used`
is the count with `"cascades"` and 0 otherwise, so the code that read the old
count reads the method through it; `per_body_layers` follows. The presets set
`shadows.method` (`Preset::Method`): `"maps"` for the maps' four, `"rays"` for
the rays'.

**"none" draws nothing.** `color_mode = 3` drew every shadow layer and then
set `shadow = 1.0` in the fragment shader. Now no layer is planned
(`n_body_layers = 0` in `Window::update`), the array is put away to 512 as
with the rays (`shadow_side`), the penumbra pass is not wanted, and the
shaders take "no layer and no rays" to mean no shadows (`shadowless()` in
`mesh_shadow.wgsl`): the main pass looks nothing up, walks nothing, and
leaves out the horizon map and the dust's transmission -- everything that
stands between the Sun and a point. The per-facet query answers by facing
alone, 0 for a point facing the Sun and 1 for one facing away, as the maps'
query does for those (`facet_sun` returns 0 there): the TPM takes the
shadows the image shows, none. A reference image is traced with its shadows
whatever the method (`Config::shadows_on`), as it was whatever `rays` said.

## `shading.color_mode` by name

`ColorMode`: `"shaded"` and `"data"`, its `code()` the 0 and 1 the shaders
always took. The user counted two modes once 3 went; there were three -- 2 was
the flat `shading.color`, a silhouette, which nothing in the examples or tests
used. Kept as `"flat"` at first and offered for removal: "flat color mode is
pointless, there is only shaded or data". It went, and `shading.color` with it
-- nothing else read it; the uniform keeps its slot, unwritten, so every
shader's copy of `Globals` still binds, and `config_renames.py` no longer
forwards the old flat name `config.color`.

The numbers were released, and every data example set `color_mode = 1`, so
Python still takes them for a release, with a `DeprecationWarning` naming the
name (`set_color_mode` in `src/py/app/config.rs`, the field `:py_custom:`).
3 sets `shadows.method = "none"` and `"shaded"`; 2 is refused, saying the flat
mode is gone and `"data"` shows each mesh's own colours. A 0 after 3, which in the
old world meant shadows back on, sets `"maps"` again when the method is
`"none"` -- `tests/test_pcf_filters.py` and `test_pcf_relief.py` toggled 3
and 0 to get an image without shadows to compare with, and a script that does
would otherwise keep them off. The examples write the names now.

## The panel

`gen_config_panel.py` learned three things for this:

- a `:when:` of clauses joined by `&&`, each about its own group's field or,
  named first, another group's (`method.uses_maps() && light.sun_as_point ==
  false`) -- a dotted first word is another group only when it is one, so a
  method of the field's own (`method.uses_maps()`) is not taken for one;
- an enum's variants by their own names in a `:when:`
  (`method == ShadowMethod::Rays`), qualified in the generated code;
- an enum's list gives each variant its doc's first paragraph as its hover.

So under the method only its settings show: the maps' resolution, PCF, biases
and levels of detail with the maps or the cascades; `per_body`, `near_layer`
and `cache` with the maps; the count with the cascades; `ray_samples` and
`ray_probe` with the rays; `access_shadow_map` always.

The Shadows topic (`shadows_ui` in `simulation_panel.rs`) shows in the shaded
mode only, as asked -- with one exception the request did not foresee: the
per-facet shadows the thermophysical model reads follow the method whatever
the image shows, and a data map of temperatures is exactly when they are
read. So while `access_shadow_map` is on, the settings stay in the data mode
too, under a line saying they are the per-facet shadows'. Otherwise a line
says shadows show in "Shaded".

## The cascades removed, "quick" made quick

The user, trying the methods: "CSM seems to be slower and less accurate than
shadow mapping. It is supposed to be ok to be less accurate only if it is
much faster to render". Measured -- once the main pass's ray code was out of
the way, which had halved everything on the M1 Pro
(`2026-10-12_ray_tracing_on_metal/`) -- it was slower in both views, frames a
second:

| | maps | none | cascades, 3 x 4096, PCF 2 | cascades, 3 x 1024, PCF 1 | one layer, 2048, PCF 1 | maps, 2048, PCF 0 |
|---|---|---|---|---|---|---|
| the Didymos pair from 1 km, 1600 x 1000 | 685 | 1421 | 479 | 595 | 901 | 868 |
| Dimorphos from 68 m, 3234 x 1774 | 201 | 686 | 127 | 168 | 179 | 331 |

"isnt it possible to improve the CSM method?" Not into something much quicker
here. Up close the maps' 3.5 ms is mostly their lookups -- 3.1 ms of PCF over
5.7 million pixels, the shadow pass 1.15 -- which cascades pay as much; and each
cascade draws every body (3.2 ms of shadow passes at 1024, 6.3 at 4096). What
cascades do in a game's world, putting texels where the camera looks, a layer
per body fitted to what the camera sees of it, with the near layer up close,
does already. What is much quicker is hard edges at 2048. The user chose:
the method gone, and `"quick"` that.

`ShadowMethod::Cascades`, `shadows.cascades`, `cascade_region`, the cascade
choice in `shadow_layer_at` and in `cs_facets`, and `tests/test_cascades.py`
went; `Shadows::per_body_layers` was `per_body` again; the uniform keeps the
count's slot (`_spare`). `"quick"` is a layer per body at 2048, PCF 0, a point
Sun. The presets, measured after it:

| | Didymos pair from 1 km | Dimorphos from 68 m |
|---|---|---|
| `"quick"` | 879 | 339 |
| `"fast"`, nothing moving | 1078 | 239 |
| `"point"`, the defaults | 692 | 204 |
| `"accurate"` | 161 | 66 |

## The reference image's box

It was a checkbox named `enabled` under "reference", its hover starting
"Integrate the image to a reference". It is **render** under "reference
image" (`:label: render`), its hover saying what it is for first: a still
drawn as exactly as kalast can, for a figure or to check a faster setting
against, sharpening every frame until its error is under `error`, then held.
The line under it gives the samples a pixel and the error it stops at, and
says when the GPU has no ray queries, its shadows then the maps'.

## Slow frames and the UI app

"when the HUD has low FPS like 10 or less the UI is unusable cus mouse is
laggy and teleports" -- the user had rays or a reference running. The panels,
the mouse and the scene share one loop, so every frame the panels wait for the
scene's: rays across the disc on the M1 Pro, 0.14-0.4 s. The user chose three
remedies: the paused scene drawn only on change, a preview while the camera
moves, and cheaper rays as an option (`2026-10-12_ray_tracing_on_metal/`,
`shadows.ray_guide`).

**A preview while the user works the window.** `last_input` in `App`, set by
the pointer, the wheel, the keys, touch and gestures -- not by a window's own
resizes and uncovers, which arrive as it opens: counted, a script stepping
fast had its first 300 ms previewed, and a reference summed nothing through
them. Within `PREVIEW_HOLD` (0.3 s) of it, or while the controller moves the
camera (a key held sends nothing between its repeats), and when the last
exact frames took longer than 1/30 s, the app asks the window for a preview
(`Window::preview`). The window makes one only of an image that traces and
nothing to export (`previewing`): the main pass by the untraced pipelines
`render::Pass::untraced` keeps beside the traced ones -- the ray code there
and untaken halves the pass, so a preview with it would be half as quick --
the maps drawn, with the disc's penumbra pass, no second depth layer; a
reference image neither summed, jittered nor shown. The per-facet shadows
stay traced. With the rays the shadow array keeps up to 2048 a side for it
(it was 512). Forced on every frame, the Didymos pair's disc drew in 2.2 ms
against 384 traced; `tests/test_image_rays.py` passes so, its exported frames
exact.

**Paused, drawn only on change**, in the UI app: no input within the hold, no
camera move, the last frame exact and into the target there is now
(`Window::drawn_generation`, made anew by a resize or a rebuild), nothing
asked of the frame (an export, a per-facet query, a pick) and no reference
summing -- then neither `Window::update` nor `render` runs, nor the queries,
whose answers are the last frame's still; the panels draw onto the scene as
it was, and the loop sleeps to the next present. Once a second
(`IDLE_REDRAW`) it is drawn anyway, for what changes without input: a level of
detail finished on its thread. The frame rate counts the frames that draw.

Not seen on screen: a window opened behind the others, as tests open them, is
occluded, and macOS gives it a frame a second, whatever the app does. Asked
of the user to try.

## The console

The python tab ran its lines in the running script's own globals -- by
design, to read and set the script's variables. `examples/cube/light.py`
sets `a` every frame (`a = 0.001 * app.simulation.state.iteration`), so the
console's `a = mesh` lasted until the next frame. Typed directly,
`app.simulation.bodies[0].mesh.facets[4]` worked, which is what made it look
like completion had broken.

The console has its own namespace now (`_console_namespace` in
`kalast/editor.py`): the script's variables copied in before each line and
each Tab, except the names the console bound itself, which `symtable` finds
in each statement it runs -- assigned, defined, imported, a loop's or a
`with`'s. A name deleted, or not bound because the line failed, shows the
script's again. A new run starts it afresh, as the script is. What it gives
up: assigning a script variable from the console no longer reaches the
script; mutating the objects it holds still does, `app` included.
`tests/test_console_namespace.py`.

## Tests

`tests/test_shadow_method.py`: a plate and a wall, the Sun 25 deg up. Maps:
the 48 facets behind the wall hidden. None: no facet of the plate hidden, the
wall's facets facing away hidden either way, and the image the maps' outside
the shadow to the level, 14,694 pixels lit inside it, none dim. The colour
mode's names and numbers with their warnings. On the M1 Pro the suite's 44
scripts pass, and the Rust tests' 334.
