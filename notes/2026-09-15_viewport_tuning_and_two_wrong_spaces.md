# Tuning the viewport, and two things measured in the wrong space

A long session of visual tuning against Blender, driven entirely by "this
looks wrong" and answered, in the end, by rendering frames and measuring
pixels. Two of the four real bugs were **not** what the complaint described,
and neither would have been found by adjusting the thing being complained
about.

## The one that cost the most: dimming in linear space

*"grid color should be darker"*, then *"still too bright"*, then *"both grid
and subgrid are still way too bright"*.

Three rounds of halving the grid colours and the picture barely changed. The
numbers were doing what they said -- `rgb * alpha` fell from 0.16 to 0.11 to
0.048 -- and none of it showed.

**The surface is sRGB** (`window.rs` picks a format with `is_srgb()`), so the
shader writes *linear* and the hardware encodes for display. Linear 0.048 is
**0.245 on screen**. A 2x linear cut is about a **15 % perceived** change, so
dimming by eye converges at a crawl, and three rounds of it looked like
nothing happening.

Found by differencing two rendered frames -- `axes = "blender"` against
`axes = "off"` -- masking the gizmo and separating the coloured axis lines by
saturation. The grey grid pixels peaked at **0.412** where the model capped at
**0.140**, and sRGB-encoding 0.140 gives **0.416**. The discrepancy *was* the
gamma.

The procedure inverts: pick what the line should look like, convert with
`((s + 0.055) / 1.055) ** 2.4`, divide by the alpha. Recorded in `CONFIG.md`
beside the values, because anyone tuning these by eye hits the same wall.

**And the ratio mattered more than the levels.** With the values finally
correct the complaint became *"grid and subgrid are indistinguishable"* --
0.10 against 0.15 is 1.5x, which at those brightnesses the eye does not
separate, so every tenth line looked like every other line. 3.5x reads as two
kinds of line.

## The second wrong space: measuring a facet by its smallest height

The wireframe turns a distant body into a sheet of wireframe colour: past
about a pixel per facet the three edges cover the whole triangle, so a
shadowed sphere comes out grey instead of black. The fix is the grid's, minus
the cross-fade -- there is no coarser wireframe, so it fades to nothing.

First version faded *"too aggressive with angle and distance"*. Two separate
mistakes, and separating them needed the coverage recovered rather than the
pixels compared.

**Angle.** `1 / fwidth(bary.i)` is a triangle's height from vertex `i`, so
three of them are its three heights, and I was taking `max` of the derivatives
-- the **smallest** height, which is exactly what foreshortening collapses.
Taking the largest instead: tilt squashes one screen direction and leaves the
perpendicular one alone. Coverage across a sphere's disc went from halving
past `r/R = 0.6` to flat -- **0.770, 0.778, 0.777, 0.775, 0.782** centre to
limb.

**Distance.** The 3-12 px window was far too wide. A 5120-facet sphere six
units out has ~5 px facets, so the *whole body* sat inside the ramp. The wash
only happens at one or two pixels; at four a line is a line. 1-4 px.

**The measurement mistake underneath both.** Comparing rendered pixels
confounds the fade with how much the shaded surface happens to differ from the
wireframe colour, which varies across a sphere -- so the first radial profile
showed a fall that was mostly shading. Inverting the blend recovers the
coverage exactly:

    edge = (shaded - on) / (shaded - wire)

Both `shaded` and `on` are renderable, and `wire` is known. That is what
showed the profile was flat.

## Two that were missing rather than wrong

**A plane view had no grid.** The shaded grid intersected `z = 0` and nothing
else, so looking along X showed the XY ground edge-on with nothing behind the
body. It now takes a plane index and uses whichever of the three faces the
camera, with the two in-plane axis lines chosen to match -- which is why
`grid_axis_z_color` had to exist.

**And no zoom.** Scrolling moves the eye along its own view direction, which a
parallel projection ignores entirely, so the wheel was dead in every
orthographic view. It scales the projection extent now, pinning it on the
first notch and releasing it on the way out. Two Rust tests, one per mode.

## Smaller, and one policy

- The grid was **re-ruled by the scene**: its base spacing came from the tick
  step, which follows the scene bounds, so a body orbiting the origin
  redrew the ground. The shader's level search was clamped at zero, which is
  what forced `spacing` to be scene-sized; unclamped, it is a *unit* and any
  fixed value gives the same grid. Fixed at 1.0.
- **Orthographic is borrowed, not kept.** Clicking a gizmo ball still switches
  to it -- a plane view in perspective is not measurable -- but the previous
  projection is remembered and restored on the first rotation away. A glance
  down an axis leaves nothing behind to undo.
- `ambient_strength` **0.002 -> 0**. Small enough to look like nothing, large
  enough to floor every dark pixel, and it feeds the `0..1` diffuse map that
  people read numbers off.
- Three gizmo settings removed (`gizmo_margin`, `gizmo_label_size`,
  `gizmo_label_color`) and one added (`grid_axis_z_color`). A letter is sized
  and coloured by the ball it sits on; two settings for one proportion can
  only agree or disagree.
- `wireframe_fade` is **off by default** -- asked for as a toggle, undecided.
  It sits in the `Globals` slot `value_mode` left behind when it was removed,
  so the uniform layout does not shift. That struct's comments record being
  bitten by exactly that twice.

## The process note

**Five window-opening tests never set `open_in_background`**, so every suite
run stole the keyboard from whoever was working. That is recorded in the
project's own memory as a preference and I had been running them in the
foreground all session. Fixed at the source in all five, and the runs moved to
the background.

The lesson that generalises: *a complaint about appearance is a measurement
problem*. Every one of these was settled in minutes once a frame was rendered
and differenced, and not before -- and in two cases the thing measured had to
be corrected before the answer appeared.
