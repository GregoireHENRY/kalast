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

## The reference image's box

It was a checkbox named `enabled` under "reference", its hover starting
"Integrate the image to a reference". It is **render** under "reference
image" (`:label: render`), its hover saying what it is for first: a still
drawn as exactly as kalast can, for a figure or to check a faster setting
against, sharpening every frame until its error is under `error`, then held.
The line under it gives the samples a pixel and the error it stops at, and
says when the GPU has no ray queries, its shadows then the maps'.

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
