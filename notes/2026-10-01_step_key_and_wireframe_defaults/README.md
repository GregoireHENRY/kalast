# 2026-10-01 — `K` steps a script's own loop; the wireframe's defaults

## `K` and Step: granted at the end of the frame

Reported: holding `K` over `examples/sphere/main.py` moved the iteration
counter and never turned the sphere. That script drives its own loop -- `while
app.running:`, doing its work only while `is_paused` is false, between two
`app.step()` calls. `app.step()` pumps events until a frame is drawn, and `K`
arrives in that pump *before* the frame's `begin_frame`: the frame saw the run
unpaused, ran the iteration itself (`update`, the hold firing in `advance`) and
returned paused. The loop never saw an unpaused moment. The Step button worked:
the UI is drawn after `begin_frame`, and the frame was already gated on the
pause state it started with.

Both now call `State::request_step`, and the frame grants it at its very end
(`grant_step`, after the `update` gate): the next frame runs the iteration and
holds after it, and a driven loop gets exactly one turn between the two.
`pause_tests::a_step_asked_for_is_the_next_frames_and_a_driven_loop_sees_it`.

## The wireframe

At the user's request, `wireframe.mode` defaults to 2, drawn over the shaded
mesh -- meshes load flat by default, which it needs -- and `wireframe.color` to
`(0.01, 0.01, 0.01, 1.0)`: very dark grey, the intent of the `0.05` the examples
set, which they no longer do. Opening a mesh in the editor no longer sets
either. The examples whose images must not carry facet edges set `mode = 0`
(the user's edits: `hera_didymos/afc.py`, `afc_eclip_didy.py`,
`didymos/main.py`). `editor_tests::reset_leaves_the_scene_as_a_new_app_has_it`
compares with the default rather than 0.

## `examples/sphere/tpm_logo.py`

The time step divides Didymos's spin exactly (`steps_per_spin` steps of
`day / steps_per_spin`): with 101 steps of the stability limit, 80.93 s, a
frame was 8,174 s, 38 s past a spin, and the body crept round 1.68 degrees a
frame. `steps_per_frame` is the user's choice apart from it. A HUD shows the
steps done in the last second, counted from a `deque` of `(time, step)` a
frame, and the progress through the run.
