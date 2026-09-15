"""Where every flat config field went when `Config` was nested, 15 September.

One table, three consumers: the migration that rewrote the examples and
tests, the deprecation shim `gen_bindings.py` emits so old names keep working
for a release, and anyone reading an old script or note. The generators that
build the panel and the bindings do *not* use it -- they read the nested
struct directly, which is the point of nesting.

`APP` means the field moved to `app.config`, the application's own config,
where a window property belonged all along.
"""

# old flat name -> (group, new name). Group "APP" is app.config.
RENAMES = {
    # debug
    "debug_app": ("debug", "app"),
    "debug_window": ("debug", "window"),
    "debug_window_mesh": ("debug", "window_mesh"),
    "debug_simulation": ("debug", "simulation"),
    "debug_depth_show": ("debug", "depth_show"),
    "gpu_timing": ("debug", "gpu_timing"),
    "occlusion_queries": ("debug", "occlusion_queries"),
    "extra": ("debug", "extra"),
    # shading
    "color": ("shading", "color"),
    "color_mode": ("shading", "color_mode"),
    "srgb_mode": ("shading", "srgb_mode"),
    "gamma": ("shading", "gamma"),
    "msaa": ("shading", "msaa"),
    "render_back_face": ("shading", "render_back_face"),
    "background": ("shading", "background"),
    # light
    "light_color": ("light", "color"),
    "ambient_strength": ("light", "ambient"),
    "light_cube_scale": ("light", "cube_scale"),
    "debug_light_cube_show": ("light", "cube_show"),
    "debug_light_cube_fit": ("light", "cube_fit"),
    # shadows
    "shadow_resolution": ("shadows", "resolution"),
    "shadow_pcf": ("shadows", "pcf"),
    "shadow_normal_offset_scale": ("shadows", "normal_offset_scale"),
    "shadow_bias_scale": ("shadows", "bias_scale"),
    "shadow_bias_minimum": ("shadows", "bias_minimum"),
    "shadow_per_body": ("shadows", "per_body"),
    "access_shadow_map": ("shadows", "access_shadow_map"),
    # wireframe
    "wireframe_mode": ("wireframe", "mode"),
    "wireframe_color": ("wireframe", "color"),
    "wireframe_width": ("wireframe", "width"),
    "wireframe_fade": ("wireframe", "fade"),
    # selection
    "selection_color": ("selection", "color"),
    "facet_labels": ("selection", "labels"),
    "facet_labels_max": ("selection", "labels_max"),
    "facet_label_size": ("selection", "label_size"),
    "facet_label_color": ("selection", "label_color"),
    # data colouring
    "value_min": ("data", "value_min"),
    "value_max": ("data", "value_max"),
    "colormap": ("data", "colormap"),
    # axes and gizmo
    "axes": ("axes", "style"),
    "axes_color": ("axes", "color"),
    "axes_ticks": ("axes", "ticks"),
    "axes_unit": ("axes", "unit"),
    "axes_label_size": ("axes", "label_size"),
    "axes_label_color": ("axes", "label_color"),
    "gizmo_anchor": ("axes", "gizmo_anchor"),
    "gizmo_size": ("axes", "gizmo_size"),
    # grid
    "grid": ("grid", "enabled"),
    "grid_width": ("grid", "width"),
    "grid_major": ("grid", "major"),
    "grid_color": ("grid", "color"),
    "grid_major_color": ("grid", "major_color"),
    "grid_axis_x_color": ("grid", "axis_x_color"),
    "grid_axis_y_color": ("grid", "axis_y_color"),
    "grid_axis_z_color": ("grid", "axis_z_color"),
    "grid_fade_near": ("grid", "fade_near"),
    "grid_fade_far": ("grid", "fade_far"),
    # hud
    "hud_font": ("hud", "font"),
    # export
    "export_dir": ("export", "dir"),
    "export_sync": ("export", "sync"),
    "export_max_queued": ("export", "max_queued"),
    "export_hud": ("export", "hud"),
    # controls
    "sensitivity_move": ("controls", "sensitivity_move"),
    "sensitivity_look": ("controls", "sensitivity_look"),
    "sensitivity_rotate": ("controls", "sensitivity_rotate"),
    "sensitivity_zoom": ("controls", "sensitivity_zoom"),
    "emulate_middle_button": ("controls", "emulate_middle_button"),
    # the image being rendered, as distinct from the OS window
    "width": ("image", "width"),
    "height": ("image", "height"),
    # window properties that were on the simulation config by accident
    "title": ("APP", "title"),
    "fullscreen": ("APP", "fullscreen"),
    "vsync": ("APP", "vsync"),
}

# The colour bar was a struct all along, flattened onto Config for Python as
# `colorbar` (its `enabled`) and `colorbar_<field>`. Those un-flatten.
COLORBAR_FIELDS = [
    "enabled", "anchor", "x", "y", "length", "thickness", "vertical",
    "label", "ticks", "text_size", "text_color", "border",
]
for _f in COLORBAR_FIELDS:
    RENAMES["colorbar" if _f == "enabled" else f"colorbar_{_f}"] = ("colorbar", _f)

# Declaration order of the groups on the nested struct, and their doc line.
# Rust type names follow the field, capitalised, except where that collides:
# `Hud` is the overlay struct, `Axes` is `app::axes::Axes`, and a struct named
# `Debug` shadows the trait in its own module.
GROUPS = [
    ("shading", "Shading", "How the surface is coloured and the image encoded."),
    ("light", "Light", "The Sun as a light: its colour, the ambient floor, the debug cube."),
    ("shadows", "Shadows", "The shadow map and its readback."),
    ("wireframe", "Wireframe", "Facet edges drawn over or instead of the surface."),
    ("selection", "Selection", "The picked facet and the facet labels."),
    ("data", "Data", "Colouring facets from per-facet values."),
    ("colorbar", "Colorbar", "The colour scale drawn for `data`."),
    ("axes", "AxesConfig", "Reference axes, tick labels and the navigation gizmo."),
    ("grid", "Grid", "The shaded ground grid of the `blender` axes style."),
    ("hud", "HudConfig", "The text overlays."),
    ("export", "Export", "Frame export."),
    ("controls", "Controls", "Mouse and keyboard sensitivities."),
    ("image", "Image", "The size of the image being rendered, as distinct from the window."),
    ("debug", "Diagnostics", "Diagnostics and console output."),
]
