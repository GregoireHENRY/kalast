# Where a point lands in the image

Asked for: the pixel position, from `(0, 0)` to the camera's resolution, of
each body's centre and of each selected facet's centre, exported from an
example. Added as a projection through the camera, the inverse of the
picking ray, and written to a CSV by `examples/hera_didymos/afc.py`.

## The engine

`Eye::project(point, size)` in `src/app/frame.rs`: the point through
`view_proj(width / height)` -- the matrix the frame is drawn with, rebuilt
from the same camera -- then the divide by `w` and NDC to pixels, `y`
flipped. Pixels from the top-left corner, `x` right and `y` down, pixel
`(i, j)` covering `i..i+1` by `j..j+1`. That is the convention `facet_pick`
already uses when it casts its ray through a pixel's centre, `i + 0.5`,
and the one the facet labels are placed with, so the three agree.

`None` only behind a perspective eye, where the divide would mirror the
point into the frame. An orthographic eye has `w = 1` everywhere and its
fitted near plane can be behind it, so it always answers. A point outside
the field of view gets its position outside `0..width`: "just left the
frame" is information.

`Simulation::project`, `project_body`, `project_facet` compose it with the
body's matrix. A body's centre is the origin of its own frame -- what
`mat[:3, 3]` holds, the SPICE position for a body placed from SPICE, and
what `anchor_body` already means by a body's position. A facet's centre is
`Facet::pos`, the mean of its corners.

The one thing the camera does not carry is the image size, which only the
window knows: `config.image` is a request whose `(0, 0)` follows the window,
and the editor draws at its viewport's size whatever it says. So
`Window::update` writes `render_size` into `Simulation::image_size` every
frame, next to where it builds `view_proj` from the same numbers.

Computed on request, on the CPU, from state that is already there: nothing
is added to the frame. It describes the frame on screen when read after the
draw -- `after_render`, or after `step()` -- because the pointer's orbit and
the orthographic fit are applied after `before_render`, at the start of the
frame.

## Checked

- Unit tests in `frame.rs`: a point on the ray through a pixel centre
  projects back onto that centre, perspective and an off-axis orthographic
  box; the field of view reaches the top and right edges exactly; nothing
  behind a perspective eye. In `simulation.rs`: the ray through each
  projected facet centre of a rotated, translated cube picks that facet.
  A half-pixel shift and a flipped `y` each fail them.
- Against the rasteriser, two spheres, 900×700: for every facet with at
  least 30 pixels in `facet_id_map`, the projected centre falls on that
  facet (496 of 496 perspective, 747 of 747 orthographic), and its offset
  from the mean of the facet's pixel centres is +0.001/-0.002 px on average,
  sd 0.1 -- no half-pixel bias.
- The AFC example, a scratch copy, 2026-11-05 from 28 km at 1020×1020: body
  centres against a pinhole model built from the SPICE vectors alone, within
  1e-4 px at every epoch; facet centres against the id map, mean offset under
  0.05 px. Selected facets fall on their own pixel while drawn; once Didymos
  has turned them away (40 degrees in the 15 minutes between frames) they
  still get a position, on the facet in front of them.

## Not done

- Visibility. A projected facet centre says where the facet would be, not
  whether it was drawn; `facet_id_map` answers that, at the cost of a second
  geometry pass. A per-point flag would need the same pass or a ray per
  point.
- A pinned `image.width`/`height` set before the window opens is ignored:
  `Window::new` sizes the render from the window, and `apply_live_config`
  only acts on a *change* against the config the window was built with,
  which already has the pinned size. Found while testing this -- pinned at
  640×480, the frame came out at the window's 900×700. `image_size` reports
  what was drawn either way, so positions stay right; the export is the
  window's size, not the one asked for. Not fixed here.
