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

**The pinch was nearly dead, and had been all along.** `PinchGesture` was
wired to `zoom(delta)`, i.e. its magnification fed in as wheel notches. A
notch is 12% of distance; a whole pinch sums to about `+1.0` of magnification,
so it moved the eye 11%. `Controller::pinch` now maps `ln(1 + m)` onto the
geometric zoom, which makes a pinch that doubles the spread halve the distance
-- exact, and unit-tested. If it feels strong, `controls.sensitivity_zoom`
scales it along with the wheel.

Direction: with macOS "natural" scrolling on -- the default -- a swipe's pixel
deltas carry the same sign as a pointer drag in the same direction, so the
scene follows the fingers exactly as it follows an ⌥-drag. With natural
scrolling off it runs the other way, which is what Blender's "Natural
Trackpad Direction" corrects; no switch here yet, since winit does not report
`isDirectionInvertedFromDevice` and one would have to be set by hand.

Not done, deliberately: the two-finger twist (`RotationGesture`, azimuth in
Blender) and `ctrl` / `shift` + wheel panning on a mouse. Neither was asked
for and each is a few lines on the same controller if wanted.
