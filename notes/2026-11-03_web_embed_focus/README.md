# kalast.greghy.com in a frame: the focus, the wheel without it, the panels shown

Three reports from the user on their site's Kalast tab, relayed by work-edds:

- "also the kalast tab on tiri web is freezing when you loose focus and is not as smooth to work on as on the kalast
  web page directly";
- "kalast tab i have to click to focus";
- "kalast tab show top and bottom panel too".

Same day as the pinch (`notes/2026-11-03_web_pinch/`), whose cause explains the first two reports.

## Why it froze

winit's web backend gives kalast these only while the canvas has the focus:
- the wheel, a swipe and a drag's motion, which are the camera's input (`DeviceEvent::MouseWheel`,
  `DeviceEvent::MouseMotion`, under `DeviceEvents::WhenFocused`, the default);
- the modifiers (`ModifiersChanged`).

In a frame, a click anywhere on the page around takes the focus, for example on its tab bar or its model menu. The
scene then stopped answering until clicked again. Keys went to the page around.

When the page around gives the frame back the focus (`iframe.focus()`), Chrome focuses no element in the frame:
its active element is `BODY`. So the canvas, and winit with it, still had no focus.

On a page of its own, nothing else takes kalast's focus so easily.

## What kalast does in a frame (`?embed=`)

- **The wheel, a swipe and a drag reach the camera whether the canvas has the focus or not.** `App::resumed` calls
  `listen_device_events(DeviceEvents::Always)` when embedded. They still go through `device_event`'s routing: the
  camera's while the pointer is over the scene.
- **The canvas takes the focus** when the pointer moves over it, and when the page around gives the frame the focus
  (`web/embed.js`, the window's `pointermove` and `focus`). Keys then reach kalast without a click, as on a page of
  its own. A field of the page around loses the focus when the pointer comes over kalast; on the Kalast tab there
  is only a model menu.
  - work-edds measured that a frame from another origin can take the focus this way without a click (headless
    Chrome 156).
  - The page around gets no pointer event as the pointer crosses into the frame, so only kalast's page can do it.
- **The toolbar and the log are shown**, as kalast.greghy.com shows them. They had been folded since 1 November
  (`notes/2026-11-01_web_embed/`). The dark theme and the meshes dressed to be looked at stay.

kalast.greghy.com on its own and the desktop app are unchanged.

## Smoothness

Nothing in `?embed=` caps the frames or changes the resolution: it sets only the theme and the mesh's dressing.

The TIRI page's own quick view (three.js) stops its loop while it is hidden.

What made the frame less smooth than the page alone:
- Most likely, it shared a process with the page around in tabs opened before both sent `Origin-Agent-Cluster: ?1`.
  kalast has sent it since 1 November, and tiri.greghy.com since today. A new tab is needed: a reloaded old tab keeps
  the shared process.
- The focus gaps above.

## Checks

Headless Chrome with WebGPU. A local page framed kalast cross-origin, as the TIRI site's does, and had a text field
of its own. A mesh was open in kalast. The test ran on the build, then on the deployed site (`2f55e3d21195`), with the
same results:

- With the outer field focused and the pointer over the page around, the frame had no focus (`BODY`).
  `requestAnimationFrame` ran at 60 a second in the frame, as it did with the focus.
- The pointer moved over kalast: the canvas took the focus, and the outer field lost it.
- A wheel notch over the scene zoomed, with the focus, from 73,132 to 93,171 mesh pixels (×1.27).
- With the outer field focused again and no pointer move, another notch zoomed from 93,171 to 119,010 (×1.28).
- Twelve pixel wheel events (a swipe) dispatched in the frame without the focus rotated the view: the gizmo turned,
  and the field kept its focus.
- After `iframe.focus()` from the page around, the canvas had the focus, and the key `N` folded the panels.
- The forwarded pinch zoomed as before, and the page around never zoomed.
