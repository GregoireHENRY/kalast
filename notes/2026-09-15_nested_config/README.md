# The config, nested: groups, generated bindings, one panel

*"The struct config made a lot of sense when we first developed the
App/Simulation/Window structs, but now that every single option is changeable
live with live impact in the UI, the Simulation struct and the right panel
should be improved. Same topics should be gathered."*

Done, and further than the panel: the struct itself is nested, the Python
bindings are generated from it, and the panel is one panel.

## What was wrong

The right-hand panel was **two panels stacked** -- the simulation's entities
(State, Bodies, Camera, Sun) above a "Config" section generated from a flat
73-field struct. A split by *where the data lived*, not by what it was about:

- **Selection** appeared twice -- the picked facet in one, `selection_color`
  in the other. So did **Export**; the code carried a comment about the header
  collision.
- The **HUD** list was above one section and its font below the other.
- The **Sun's** own properties -- `light_color`, `ambient_strength`, the debug
  cube -- sat in a "Lighting" group nine headers from the Sun.
- `background` was under "Shading", "Window" was elsewhere, and the grid was
  orphaned after both.

## What it is now

```
app.simulation.config.<group>.<field>

shading   light   shadows   wireframe   selection   data   colorbar
axes      grid    hud       export      controls    image  debug
```

One rule: a group becomes a sub-struct, the group prefix is stripped, nothing
else is renamed -- `grid_color` is `grid.color`, `debug_light_cube_show` is
`light.cube_show`, `shadow_per_body` is `shadows.per_body`. The colour bar was
a struct all along, flattened onto `Config` for Python as `colorbar_*`; it is
`colorbar.*` now like everything else.

**Two configs stay two.** `9781ca9` split them on purpose -- `app.config.width`
is the OS window and `app.simulation.config.image.width` the image, and a 4K
export from a small window depends on that. What was wrong was only the
boundary: `title`, `fullscreen` and `vsync` are window properties and sat on
the simulation side. They moved.

**The Sun** is `sim.sun` (where it is) plus `config.light` (how it lights),
and the panel's **Sun** header shows both. Not colour hung on `sim.sun`,
because that is an `Eye`, the same type as the camera, and a camera has no
colour.

## The trap that decided how the bindings are made

Every accessor goes through a shared `Rc<RefCell<Config>>`. The obvious way to
nest -- `#[pyclass(get_all, set_all)]` on each sub-struct, the way `Hapke` and
`Spin` are done -- hands Python a **copy** when it reads `config.grid`, so
`config.grid.color = ...` sets a field on a temporary and nothing happens. No
error. That is worse than the flat struct.

So each group is a *view*: a proxy class holding the same `Rc`, reading its
own group through it on every access. That is a page of identical code per
group, fourteen times -- the current hand-written 1,426 lines, multiplied. So
they are **generated** now, by `tools/gen_bindings.py`, from the same struct
the panel and the stubs already come from. The bindings were the last mirror
still written by hand, and `CLAUDE.md` records that biting four times; this
closes it. `src/py/app/config.rs` went from 1,426 lines to 375, all of them
logic: the `Hud` class, the colormap parser, the `huds` forwarder.

Verified the way it has to be: a value set through one proxy read back through
a second, freshly created one. Same number.

The generators now agree with each other, and `tests/test_stubs.py` checks
that they do: every group view is a case, built live, with its stub compared
against `dir()`.

## Old names keep working, for one release

A `__getattr__`/`__setattr__` shim on the root, generated from
`tools/config_renames.py`, forwards `config.grid_color` to
`config.grid.color` with a `DeprecationWarning` naming the new path. The three
that moved to `app.config` raise an error that says where. Cheap, removable,
and it means the other machine's uncommitted scripts do not all break on the
next pull. `__setattr__` falls through to `PyObject_GenericSetAttr` for
anything not in the table, so the one real setter left on the root -- `huds`
-- still runs.

## The panel

One function, `simulation_panel`, composing topic headers over the entity UI
and the generated group functions:

| header | holds |
|---|---|
| Run | iteration, pause, step target |
| Bodies | the body list |
| Selection | the picked facet + `config.selection` |
| Camera | `sim.camera` |
| Sun | `sim.sun` + `config.light` |
| Shading / Shadows / Wireframe | their groups |
| Data colouring | `config.data` + `config.colorbar` |
| Axes & grid | `config.axes` + `config.grid` |
| HUD | the HUD list + `config.hud` |
| Window | `app.config` + `config.image` |
| Controls / Export | their groups, Export with the buttons |
| Debug | the visibility diagnostics + `config.debug` |

The panel generator emits one function per group and nothing else; there is no
prefix table and no `:group:` marker any more, because the struct's nesting
*is* the grouping. Which groups share a header is decided in
`simulation_panel.rs`, by hand, in one place.

## The mechanics, for the record

- Rust readers renamed by a regex over the same table: 182 in `src/app`, 12 in
  the Rust examples, then compiler-first for the rest. Two fields are prefixes
  of their own group (`grid`, `axes`), so `config.grid.width` got rewritten a
  second time into `config.grid.enabled.width` -- caught as "`bool` has no
  fields" and undone.
- Scripts and tests: 57 lines in 20 files, table-driven, plus two by hand
  where the receiver was `c.vsync` or `sim.config.vsync` and the migrator could
  not know which `app` to send it to.
- `CONFIG.md`: 81 entry headers and 75 references rewritten from the table --
  and the pass then re-hit its own output on the three moved fields, producing
  `app.app.config.title`. Un-doubled by hand; the API/CONTROLS pass excludes
  `app.config.` receivers explicitly.
- `gen_stubs.py` builds one stub from several sources now, since `Config` is
  declared in one file and gets its accessors in another under
  `impl super::config::Config`.

92 Rust tests, 14 Python files, all three generators current.
