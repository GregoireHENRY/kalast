# kalast.greghy.com: the mouse wheel zooms, as on the desktop

The user: "scroll wheel on web do rotation instaed of zoom, but in local app it works well".

kalast tells a wheel from a trackpad by the kind of scroll it gets (`Controller::scroll`, Blender's test): a wheel's
notches (`LineDelta`) zoom, a trackpad's pixels (`PixelDelta`) orbit. On the desktop the system says which
(`hasPreciseScrollingDeltas` on macOS). In a page, winit makes `PixelDelta` of every wheel event in pixels
(`DOM_DELTA_PIXEL`), and Chrome, Edge and Safari report a mouse wheel in pixels as well as a trackpad: the wheel
orbited.

`web/wheel.js`, installed first by `web/index.html`, stops such a wheel event before winit sees it (the window's
capture phase) and sends it again in lines (`DOM_DELTA_LINE`), which winit makes `LineDelta`:

- **A wheel or a trackpad:** the legacy `wheelDeltaX/Y` count 120 a notch for a wheel; for a trackpad Chromium and
  WebKit make them three times the pixels (40 pixels a tick), and such an event is left alone.
- **macOS:** a wheel's pixels are the system's lines times 40 (Chromium's `kScrollbarPixelsPerCocoaTick`), so it is
  sent again as `delta / 40` lines: the lines the desktop app gets. NSEvent's `deltaY`, which Chromium reads, and
  `scrollingDeltaY`, which winit reads, are one number: checked on this Mac with line-based scroll events made with
  a wheel's fields (fixed-point 0.1, 0.2, 1.0, 2.5 lines).
- **Elsewhere** (Windows, Linux): `-wheelDelta / 120` lines, a notch a line, as winit gives the desktop app there.
- **Left as they are:** Firefox's (lines already, or pixels without `wheelDelta`: a trackpad's), a pinch (`ctrl`, which
  zooms either way), events the page made.
  Since 3 November a pinch is still left to winit but cancelled, so the page never zooms; Safari's gesture events and
  a pinch over a frame, sent on by the page around, go to kalast as the desktop's pinch: `notes/2026-11-03_web_pinch/`.

The cases (a slow and a fast notch and a shift-wheel on macOS, a trackpad, a notch on Windows at 100 % and 125 %,
Firefox's line and pixel events, a pinch) are worked out from Chromium's and WebKit's conversion of the system's
events, not measured on each device.

**Corrected the same evening.** The user: "zoom on web is not behaving like on app". The first rule took a wheel on
macOS by its pixels being multiples of 4.000244140625 (40 x 6554/65536, the step Mapbox GL looks for), and sent
any other wheel event as whole notches. But the system's fixed-point lines are rounded either way: a slow notch of
0.1 line comes as 6553/65536, 0.0999908 (measured here), 3.99963 pixels, off that step. Such events went as a
whole notch, about ten times the desktop's zoom. A wheel is now told by `wheelDelta` alone, and on macOS sent as
its pixels over 40 whatever they are.

## Checks

In headless Chrome with WebGPU, a page framing kalast with a mesh open, three trusted wheel events (CDP: `deltaMode`
0, `deltaY` -100, `wheelDeltaY` 120, as a Windows notch):

- the site before: the mesh orbited, the gizmo with it, its pixels on screen 61,434 then 30,567;
- with `wheel.js`, local and then deployed: three line events of -1 reached winit, the mesh came closer facing the
  same way, 61,434 pixels then 146,214.
