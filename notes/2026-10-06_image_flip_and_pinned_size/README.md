# 2026-10-06 — the image's mirrors, and a pinned image size that holds

Asked while comparing `examples/hera_mars_swingby/tiri_diffuse_light.py` with
a TIRI browse image (`edds_decoder/out/browse_par/20250312/...__1_0.png`,
1018 x 768): the render looked flipped on Y, `up = -Y` did not cure it, and
the export was 3360 x 1774 for a 1024 x 768 request.

## Why the TIRI image is a mirror

A camera looking along +Z with up +Y (`look_to_rh`) has +X on its left: right
is `dir x up = -X`. SPICE at 12:08:58: Deimos, 4.15 deg toward +X, lands at
column 215 of the browse PNG, on the left; Mars's centre, 0.54 deg toward -Y,
sits above the middle, where its night side bulges (about row 340). The PNG
runs its columns toward -X and its rows toward +Y: pixel (0, 0) at the
(+X, -Y) corner, a mirror of the view -- its right x up points along the
boresight -- which no `up` reproduces: `up = -Y` turns the image by 180
degrees. TIRI's IK (`hera_tiri_v03.ti`) draws the view with +X right and +Y
down and puts its pixel (0, 0) at the lower left, the opposite corner from the
PNG's.

## `image.flip_x`, `image.flip_y`

In `Projection` (`flip`, copied from the config in `apply_live_config` every
frame), as a scale of -1 on the projection matrix's x or y. The render,
`project`, `ray_through_ndc`, the facet-id pass and the axes all go through
`Eye::view_proj`, so they follow without anything else. One mirror and not
the other turns the winding round on screen: the scene pass culls
`Face::Front` then (`pass/render.rs`), and `Realised.mirrored` rebuilds the
passes when that changes, as `render_back_face` does. The shadow pass is the
Sun's, never mirrored. The gizmo builds its screen positions from the camera's
basis and takes `Projection::mirror()` there. The arcball's orbit and pan, and
WASD's keys and look, take the same signs, so a drag still moves the image the
way the pointer goes. The HUD and the colour bar are drawn in screen space
after the scene, and stay readable.

Rendered against numpy's own flips -- TIRI's scene at 12:08:58, 1018 x 768:
each single mirror within 0.10/255 on average of the plain frame flipped
(0.1 % of pixels off by more than 10, the rasteriser's tie-breaking on
mirrored edges), both mirrors exactly, the mean the same in all four, so no
face was lost to the culling. With `flip_y` and `up = +Y` the frame has the
browse image's orientation; Deimos lands about 30 px from where TIRI saw it,
at the time the user set.

Tests: `a_mirrored_image_puts_the_view_the_other_way_round` (the view's top
right at the top left, the bottom right and the bottom left, and a click's ray
through what is drawn there), `the_balls_are_mirrored_with_the_image`.

## A pinned image size

`image.width` and `image.height` were ignored twice. In the editor the render
size was set from the viewport panel every frame, whatever was pinned -- the
code said so, while `docs/CONFIG.md` said the editor "has no such limit". In a
plain window, `Window::new` took the window's size and `apply_live_config`
took the config it found at the first frame as already applied, so a size set
before the window opened -- every script's -- never was; `resize` also sized
the targets to the window. Now `Window::new` and `resize` use `image_size`,
the pinned size or the window's, and the editor renders a pinned image at its
size and paints it fitted, letterboxed, as `cursor_in_image` already mapped
the pointer, with `viewport_scale` 1 so `fovy` spans the image. Checked: a
plain window and a test editor (its settings redirected with
`KALAST_SETTINGS` into the scratch space) both export 1018 x 768.

Not added: a high/normal DPI switch for an image that follows the window. A
pinned size is pixels whatever the screen; offered if still wanted.
