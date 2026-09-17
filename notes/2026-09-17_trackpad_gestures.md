# Trackpad gestures: Blender's map, and how Blender tells the devices apart

Orbiting needed a click with ⌥ held, which on a trackpad is a thumb on the
key, a finger pressing and a second finger dragging. Blender orbits with a
two-finger swipe and zooms with a pinch; the question was how it decides what
a scroll event means, since a wheel notch and a two-finger swipe both arrive
as "scroll".

## What Blender does

It decides at the OS layer, not in the keymap. In `GHOST_SystemCocoa.mm` a
`scrollWheel` `NSEvent` with a gesture phase and precise (pixel) deltas becomes
a **trackpad** event (`GHOST_kEventTrackpad`, subtype scroll); one without --
a physical wheel -- becomes a **wheel** event. Magnify and rotate gestures are
trackpad events of their own subtypes. The window manager turns those into
distinct event types, `MOUSEPAN` / `MOUSEZOOM` / `MOUSEROTATE` /
`MOUSESMARTZOOM` against `WHEELUPMOUSE` / `WHEELDOWNMOUSE`, and the default
keymap binds them separately:

| Event | Bound to |
|---|---|
| wheel up / down | `view3d.zoom`, one step per notch |
| `MOUSEPAN` (two-finger swipe) | `view3d.rotate` |
| `shift` + `MOUSEPAN` | `view3d.move` (pan) |
| `ctrl` + `MOUSEPAN` | `view3d.zoom` |
| `MOUSEZOOM` (pinch) | `view3d.zoom` |
| `MOUSEROTATE` (two-finger twist) | `view3d.rotate`, azimuth only |
| middle-drag | `view3d.rotate` (modal, until release) |
| `alt` + left-drag | the same, only with "Emulate 3 Button Mouse" |

Two things worth copying beyond the table. The trackpad operators are
**non-modal**: each event applies its delta and finishes, where a mouse drag is
one modal operator between press and release. And the trackpad deltas go
through the *same* rotation code as the mouse, `prev_xy + delta`, so a swipe
orbits exactly as far as a drag of the same length. The preferences on top:
"Emulate 3 Button Mouse" (kalast's `controls.emulate_middle_button`), "Natural
Trackpad Direction" and "Multitouch Gestures", the last of which turns the
swipe back into wheel emulation.

## What winit gives us

The same discriminator, already. winit's macOS backend builds
`MouseScrollDelta::PixelDelta` when the event `hasPreciseScrollingDeltas` and
`LineDelta` otherwise -- Blender's test, one call earlier. `PinchGesture`
carries the raw `magnification`; `RotationGesture` and `DoubleTapGesture`
(smart magnify) exist and are unused. What winit does *not* expose is the
momentum phase, so the events macOS keeps sending after the fingers lift are
indistinguishable from the swipe; a flick keeps orbiting a little. Blender
passes those through as well.

kalast reads scrolls from `DeviceEvent::MouseWheel`, which keeps the delta kind
and drops the touch phase -- fine, since the routing needs only the kind. It
was already gated on the pointer being over the scene, so a swipe on the
script panel scrolls the panel.

## What changed

`Controller::scroll(delta, scale_factor)` in `src/app/frame.rs` is the whole
policy: `LineDelta` zooms; `PixelDelta` goes to `drag()` (so `shift` pans, as
for the mouse) unless `ctrl` is held or `controls.trackpad_orbit` is off, in
which case it zooms at the old fifty-pixels-a-notch rate. The swipe is divided
by the window scale factor because the pixels arrive physical and a drag
arrives in points; without that a swipe on a retina panel orbited twice as
far as the drag beside it.

**The pinch was dead in the editor, and had been all along -- twice over.**
First, it never arrived. The editor gives egui first refusal on every window
event and drops what egui reports as consumed; egui-winit answers a
`PinchGesture` with `consumed = egui_wants_pointer_input()`, which is true
whenever the pointer is over *any* egui area, and the viewport is one. The
wheel escapes because `MouseWheel` is on the short list of pointer events the
editor routes by `pointer_on_scene()` instead of by `consumed`; the pinch was
not on that list, so it was swallowed every time. It is on the list now.
Second, where it did arrive -- a bare `app.start()` window with no editor --
it was wired to `zoom(delta)`, its magnification fed in as wheel notches. A
notch is 12% of distance and a whole pinch sums to about `+1.0`, so it moved
the eye 11%. `Controller::pinch` now maps `ln(1 + m)` onto the geometric zoom,
which makes a pinch that doubles the spread halve the distance -- exact, and
unit-tested. If it feels strong, `controls.sensitivity_zoom` scales it along
with the wheel.

Direction: with macOS "natural" scrolling on -- the default -- a swipe's pixel
deltas carry the same sign as a pointer drag in the same direction, so the
scene follows the fingers exactly as it follows an ⌥-drag. With natural
scrolling off it runs the other way, which is what Blender's "Natural
Trackpad Direction" corrects; no switch here yet, since winit does not report
`isDirectionInvertedFromDevice` and one would have to be set by hand.

Not done, deliberately: the two-finger twist (`RotationGesture`, azimuth in
Blender) and `ctrl` / `shift` + wheel panning on a mouse. Neither was asked
for and each is a few lines on the same controller if wanted.

## Upside down, the drag reversed

Reported the same afternoon: with the view upside down (-Z up the screen), a
drag to the right turned the scene to the left. Blender does not do that.

The turntable orbits about `up_world` with a fixed sign, so the camera always
travels to the same *world* side for a drag to the right. Upright, that side is
screen-left and the scene turns right -- grab-and-drag. Upside down, screen
right is the mirror of world right, the camera travels to screen-right and the
scene turns left. Blender has the same turntable and handles it with one
flag, `vod->reverse = -1` when `persmat[2][1] < 0` -- when world-Z projects
down the screen -- applied to the global-Z rotation only. `arcball_update`
now does the same from `up.dot(up_world) < 0`. The vertical term is untouched,
as in Blender: its axis is `right()`, the screen's own, which flips with `up`.

One difference: Blender latches the flag at the start of a drag, so a drag
that crosses the pole keeps its sign until release; here it is read each
frame, so the crossing flips at once and the scene keeps following the
pointer. Unit test: `horizontal_orbit_follows_the_screen_when_upside_down`.
