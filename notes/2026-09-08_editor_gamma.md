# The editor viewport was a gamma too dark

Reported as "the selection colour is yellow but shows orange". It was — and so
was everything else in the editor.

## The measurement

A flat 0.5 grey (`color_mode = 2`, `color = (0.5, 0.5, 0.5)`), same scene,
same camera, read off the actual pixels:

| | level |
|---|---|
| plain window | 0.502 |
| exported PNG | 0.502 |
| **editor viewport** | **0.216** |

`0.5^2.2 = 0.218`. The selection colour told the same story: `(1.0, 0.85,
0.1)` reached the screen as `#FFB302` = `(1.000, 0.702, 0.008)`, and
`0.85^2.2 = 0.699`, `0.1^2.2 = 0.006`.

## The cause

The scene is rendered into `render_texture`, whose format is the surface's —
an sRGB one. Writing to it encodes, so the texture holds **already-encoded**
bytes. That is what an exported frame contains, and it is why the exported PNG
was right all along.

The plain window blits that texture to the swapchain: bytes through, encode
once at the end, correct.

The editor hands the texture to egui instead, and registered it through
`TextureViewDescriptor::default()`. For an sRGB texture that is an sRGB
*view*, so sampling **decoded** it to linear on the way in — and nothing
encoded it again on the way out. One conversion too many, in the direction
that darkens.

Registering a non-sRGB view of the same texture passes the bytes through, the
way a blit does. `view_formats` on the render target had to allow the
non-sRGB twin for that view to be creatable at all.

After: the editor measures 0.502, the same as the other two.

## What it means for work done before today

**Anything judged by eye in the editor was darker and more saturated than the
render it stands for.** A colour chosen there to look right will look washed
out anywhere else, and a shadow or a terminator judged "too dark" in the
editor may have been correct.

**Nothing measured is affected.** Exported frames are written from the
texture, not from what egui made of it, so every number in these notes that
came from a PNG stands. So do the plain-window screenshots.
