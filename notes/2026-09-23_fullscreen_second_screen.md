# Fullscreen on the second screen, and the focus-mode toolbar's height

Two UI-app fixes from the same afternoon, both for v0.5.7.

## `F` did nothing on an external monitor

Report: "fullscreen F works perfectly on main screen but doesn't work on
2nd screen". The window went fullscreen and came straight back out, within a
frame, so nothing seemed to happen.

The cause is the green-button heuristic in `App::apply_live_config`. Native
fullscreen is turned off for the window (`macos::disable_native_fullscreen`),
so the green button zooms, and each frame `isZoomed` is read: a rising edge is
taken as "fullscreen was asked for", the zoom is undone and the simple
fullscreen put in its place. `isZoomed` is AppKit comparing the window's frame
with the screen's *visible* frame. In simple fullscreen winit sets the frame to
the screen's whole frame, and whether that equals the visible frame depends on
the screen, measured with a throwaway winit probe (an accessory app at the
bottom window level, so no focus moved):

| screen | frame | visibleFrame | safeAreaInsets.top | `isZoomed` in simple fullscreen |
|---|---|---|---|---|
| laptop (M1 Pro, notch) | 1512 × 982 | 1512 × 950 | 32 | false |
| PHL 279P1 (external) | 1920 × 1080 | 1920 × 1080 | 0 | **true** |

On the laptop the notch keeps 32 points out of the visible frame (and the
Dock its strip), so the two never match and the heuristic never fired. The
external monitor has neither: one frame after `F`, `isZoomed` read true, was
taken as a press, `unzoom` ran and `fullscreen` was toggled back off.

Fix: the zoom is not read while the window is fullscreen or on its way in or
out -- `config.fullscreen || realised.fullscreen`. The realised side matters
for the frame after `F` asks for the way out, when the config already says
"out" but the window is still the size of the screen; without it the read
would fire there instead and put the fullscreen back. There is no green
button to press in simple fullscreen anyway, the title bar being hidden.

Not reproduced inside kalast itself: the window opens centred on one monitor
and there is no option to place it on another. The measurement above is the
probe's; the fix follows from it. To be confirmed by the user on the second
screen. On the laptop screen `fullscreen` toggled on and off from a script
still holds across frames.

## The summoned toolbar looked two rows tall

In focus mode (`Shift+F`) the toolbar comes back when the pointer reaches the
top edge, as a floating `Area` framed like a popup. It was given
`FLOAT_DEFAULTS[0] = 30` points and made to fill them with `set_min_size`, so
below the one row of buttons sat a band of empty frame -- a bar half again as
tall as its docked self, which reads as two rows. It now pins only its width,
takes the row's height, and has no grab strip, since there is nothing to
drag on a one-row bar. The other three floating panels are unchanged.
