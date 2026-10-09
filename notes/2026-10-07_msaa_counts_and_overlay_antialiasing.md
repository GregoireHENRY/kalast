# 2026-10-07 — the GPU's MSAA counts; each overlay's own antialiasing

Asked: why 5x to 8x MSAA were "not supported"; 3x, 5x, 6x and 7x off the
slider; 2x usable; a console message naming the machine; then antialiasing
for the wireframe, the axes and the HUD text apart from the mesh's MSAA --
"on/off per overlay", the user's pick over a separate MSAA pass each.

## MSAA

MSAA takes powers of two. `resolve_samples` looked at the counts WebGPU
*guarantees* (`guaranteed_format_features`: 1 and 4 for every renderable
format), so 2x and 8x fell to 4x whatever the GPU. A probe on this machine,
Apple M1 Pro (Metal): `Bgra8UnormSrgb`, `Bgra8Unorm` and `Depth32Float` all
have 1, 2 and 4 -- no 8.

Now the window takes `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES` when offered,
and `MsaaSupport::of` asks the adapter once which counts both the image's
format and the depth's have, with the GPU's name. `Passes::new` resolves the
count once (it was resolved twice, in `render::Pass::new` too) and hands it to
the main pass; a count the GPU lacks falls to the largest below, said once:
`msaa: 8x is not supported by Apple M1 Pro (Metal), which has 1x, 2x and 4x:
using 4x`. In the settings, `msaa` is a list -- off, 2x, 4x, 8x -- through a new
`:choices:` marker in `tools/gen_config_panel.py` (integers; a value set from
a script that is not on the list is shown as it is).
Test: `a_count_falls_to_the_largest_the_gpu_has_below`.

## Each overlay's antialiasing

What smoothed what: the wireframe, the mesh's shader blending its edge over a
pixel; the grid and the gizmo, their shaders likewise; text, the font's
coverage; the axes, one-pixel `LineList` lines, nothing but the main pass's
MSAA. So three switches, each with its own way:

- `wireframe.antialias`: `wireframe_edge` cuts at the blend's half
  (`nearest < width - 0.5`) when off. Through `Globals.wireframe_antialias`,
  in what was `_padding1`.
- `axes.antialias`: the axes are now a strip a segment, the line list read two
  vertices an instance (`draw(0..4, 0..n/2)`), widened on screen to the line
  and a pixel past it, `across` interpolated linearly on screen; the fragment
  blends `smoothstep(0, 1, d)` or cuts at half a pixel. An end behind a
  perspective eye is brought along the segment in front of it before it is
  projected. The strip's own edges are transparent, so MSAA changes nothing.
  `Globals.axes_antialias` (was `_padding2`) and `Globals.image_size`,
  appended: the struct goes from 96 to 112 bytes, and the shaders declaring
  the shorter one still bind.
- `hud.antialias`: `wgpu_text` takes no shader of its own, so off, the text
  pass draws on a layer cleared to transparent (`pass::text_layer`, one a
  size, two at most) and a full-screen triangle copies it over, keeping a
  pixel where the coverage is half or more: `rgb / a`, opaque. Every piece of
  text drawn over the image goes through that one pass, the labels too.

Checked on a 480 x 360 render of `ico3` with the `blender` axes and a HUD, each
switch against the plain frame: wireframe hard, 16.7 % of pixels differ; axes
hard, 0.46 %, on the lines alone; text hard, 0.92 %, in the HUD's rows; enlarged,
each hard edge has no grey left. `srgb_mode` 1 against 0 in the lit mode: mean
50.4 against 83.2.

## sRGB's own curve

The image is an sRGB texture: the GPU stores what the shader writes through
the sRGB curve. `srgb_mode` picks the side the shader undoes it on, with
`pow(colour, gamma)`, gamma 2.2 -- which parts from the curve in the dark.
Measured on a cube face lit head-on, colour 0.078 (Deimos's normal albedo,
which the user is trying): mode 1 stored 12/255 for a value of 20 -- 0.047 for
0.078; 35 % of the value at 0.05, 93 % at 0.2, 101 % at 0.5. And mode 0 stored a
colormap's 0.05 as 0.018. The user had changed `srgb_mode` and seen nothing;
any value but 0 and 1 converts nothing, and lit images are then as in mode 0.

Now `srgb_to_linear` (`mesh_shadow.wgsl`, `colorbar.wgsl`) is the curve's own
decoding, which the encoding undoes exactly; `shading.gamma` is
`Option<Float>`, unset, and set it is the old power law, for reproducing an
image from before. `srgb_mode` is a list of its two modes (`:choices:`).
Measured again: lit 0.078 in mode 1, 20/255; a raw 0.05 in mode 0, 13/255;
lit 0.5 in mode 1, 127/255 for 127.5; mode 0's lit values unchanged, 79 and
188.

## Open

- A scattering law for airless bodies in the shader (Hapke, or
  Lommel-Seeliger with a phase function): Lambert is too bright away from
  opposition for Deimos -- phase integral 0.23 against a Lambert sphere's 1.5
  (Wargnier et al. 2025, A&A 703, A289). Asked for by the user, after trying
  the normal albedo (0.078) as the colour first.
