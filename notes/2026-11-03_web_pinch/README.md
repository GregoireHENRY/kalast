# kalast.greghy.com in a frame: a pinch zooms kalast, never the page

The user, relayed by work-edds (Chrome on macOS): a trackpad pinch over kalast in their site's Kalast tab zoomed the
whole page, not kalast's view. At kalast.greghy.com itself, the pinch zoomed kalast's view.

## Why

Chrome makes a trackpad pinch into wheel events with `ctrlKey`. Their `deltaY` adds up to -100 ln(scale) in pixels,
and their `wheelDeltaY` is ±120. Over a frame from another origin, Chrome gives them to the **page around the frame**,
not to the frame. Nothing there cancelled them, so the page zoomed.

This was measured in headless Chrome with `Input.synthesizePinchGesture` (a touchpad's pinch). A page framed kalast
cross-origin, with no listener of its own:
- over the frame, at four points, no wheel event reached kalast's document, and the framing page zoomed to 1.6;
- the same pinch over kalast as the page itself reached its canvas: 13-14 events, all cancelable and cancelled
  (winit's listener), and no zoom.

## The page around

The TIRI site's Kalast tab now does this:
- it catches Ctrl+wheel events over the frame and cancels them;
- it sends each one on to the frame, as it got it:
  `{tiri: "wheel", deltaX, deltaY, deltaMode, ctrlKey: true, x, y}`;
- it also cancels Safari's gesture events on that tab.

## kalast

`web/embed.js` takes the message only under these conditions:
- it comes from the parent window;
- its origin is one of those allowed for `open` (greghy.com over https, the visitor's own machine);
- it has `ctrlKey` and `deltaMode` 0 (pixels).

Each event becomes the magnification of its step, exp(-deltaY / 100) - 1. That goes to `kalast.pinch`, then to
`gui::send_pinch`, and between frames `App::page_pinch` gives it to `window_event` as a `WindowEvent::PinchGesture`.
This is the desktop's trackpad pinch, and it goes the same way:
- the UI's while the pointer is over a panel;
- the camera's while the pointer is over the scene, at the desktop's 1:1 (`Controller::pinch`).

A wheel event dispatched on the canvas, as first proposed, would have done nothing in most cases, because winit's
web backend gives kalast the wheel only while the canvas has the focus:
- the camera's zoom comes from `DeviceEvent::MouseWheel`, which winit sends only with the focus
  (`DeviceEvents::WhenFocused`);
- Ctrl comes from `ModifiersChanged`, which winit also sends only with the focus.

In the Kalast tab, the canvas has no focus until the visitor clicks in it. This was measured in headless Chrome
with kalast as the page itself. With the canvas blurred, a pinch left the camera where it was. With the canvas
focused, the same pinch moved it.

A pinch handed to `window_event` needs no focus, as on the desktop. It also takes no focus from the page around.

**Safari** reports a pinch as gesture events of its own, `gesturestart`, `gesturechange` and `gestureend`, which
winit does not read. Safari zooms the page unless they are cancelled. `web/wheel.js` cancels them from the moment
the page loads. Once kalast has started (`connect`), each event goes to `kalast.pinch` as its `scale` over the one
before (Safari's `scale` counts from the gesture's start). A Ctrl+wheel event during such a gesture is not passed
on, should Safari send both.

**Ctrl+wheel** is cancelled anywhere in kalast's page by `wheel.js`, before kalast has started too. It still goes on
to winit, as before.

## Checks

Headless Chrome with WebGPU. A local page framed kalast cross-origin and did what the TIRI site's does: it cancelled
Ctrl+wheel over the frame and sent it on. A mesh was open (`res/dimorphos_v004_10k.obj`, sent by `open`). The mesh's
pixels on screen were counted after each pinch:

| pinch | where | mesh pixels | ratio | expected |
|---|---|---|---|---|
| none | | 73,119 | | |
| 1.5 | the scene | 167,456 | 2.29 | 1.5² = 2.25 |
| 0.5 | the scene | 41,273 | 0.246 | 0.25 |
| 1.5 | the side panel | 41,273 | 1 | the UI's |
| 1.5 | the scene | 92,750 | 2.25 | 2.25 |

- With a text field of the framing page focused (the frame's `document.hasFocus()` false, its active element
  `BODY`), a pinch of 1.5 took the mesh from 73,119 to 167,456 pixels. The field kept the focus.
- Safari's gesture events, simulated in the frame (Chrome sends none), with scale rising to 1.5: all cancelled, and
  the mesh went from 167,456 to 395,293 pixels (2.36).
- The framing page's `visualViewport.scale` stayed at 1 throughout.
- kalast as the page itself: Chrome's own pinch is cancelled at the canvas as before, and the page does not zoom.
- The deployed site (page version `fb5f7e167ac4`), framed by the same local page: the same pixel counts.

## Open

- **kalast.greghy.com itself:** Chrome's, Edge's and Firefox's own pinch still goes winit's way, as Ctrl and a swipe.
  - It acts only while the canvas has the focus.
  - It zooms `2 x scale factor x ln(scale)` notches, against the desktop's `ln(scale) / 0.12`: a quarter of the
    desktop's zoom at scale factor 1, half at 2. In headless Chrome (scale factor 1), a pinch of 1.25 brought the
    eye 5 % closer, where the desktop brings it 20 % closer.
  - Telling a pinch from Ctrl and a mouse wheel would take the deltas: both have `wheelDeltaY` ±120.
- **Safari:** not tried in Safari itself. Over a frame from another origin, it is not known whether Safari gives its
  gesture events to the frame or to the page around.
