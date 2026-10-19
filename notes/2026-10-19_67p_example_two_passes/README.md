# Comet 67P's example in two passes; each image's pointing fitted in kalast

19 October 2026, following `2026-10-16_67p_second_pass/`: what Friday ran
from scratch scripts, now in kalast and `examples/comet67p_tiles`.

## `kalast.shape.pointing`

`src/shape/pointing.rs`. A maplet's solve registers every image to it, and
the shift is the image's pointing plus the maplet's own offset seen through
the image's projection, `s[k, t] = P[k] + J[k, t] o[t]`. Every image is in
dozens of maplets and every maplet has dozens of images: both are fitted to
all the shifts at once -- each maplet's offset by weighted least squares
held toward zero (`offset_sigma`), each image's pointing the median of what
its maplets leave, alternated; Huber past `huber` pixels. What SPC and MPCD
do outside the shape fit, and what `pointing.py` did on Friday.

- **Against Friday's fit**, on the first pass's 6,489 tiles: 312,583
  registrations, 4,894 images, 0.3 s; every image's pointing and every
  tile's offset the same, to print precision.
- **Test** (`the_shifts_give_back_each_images_pointing`): 60 cameras around
  300 tiles, each image a known pointing, each tile a known offset, one
  shift in twenty a wrong lock 25-40 px off in any direction -- every
  pointing within 0.15 px, the wrong locks the residuals over 20 px. Wrong
  locks all off the same way (the test's first draft) move every tile's
  offset alike and the pointings with them, 0.7 px: registration does not
  lock wrongly in one direction, so the test does not either.

## The example

`examples/comet67p_tiles/main.py`: `fetch`, `first`, `pointing`, `second`,
`score`.

- **The plan**, 7,364 tiles: Friday's 6,332 of 241 points outside the
  turning places, in their order, then the 1,032 of 121 points in them; the
  138 places where neither size beat SHAP7 have no tile. Each row now
  carries `size`, and `low` and `high`, SHAP7's lowest and highest heights
  under the tile: `fetch` keeps every line a tile's points fall on, which
  the old box -- a quarter of the half-width either side -- did not (tile
  0's relief runs -56 to +17 m). 5,334 images, the 14 no tile uses left out.
- **`first`**, the 241-point tiles alone, each image registered on its own
  24 spacings around; **`pointing`** over all of them; **`second`**, every
  tile, each image where its pointing puts it, 3 px searched, the tile kept
  where the plan has it. The 121-point tiles have no first pass: too little
  ground to register an image on.

## Checked end to end

A copy of the example writing to `/Users/gregoireh/data/mesh/67p/tiles_e2e`:

1. `fetch` and `first`, tiles 2000-2029 from nothing: 29 of 30 bit-identical
   to the Mac's first pass, the 30th 7 mm apart at most -- one of its images'
   windows no longer cut.
2. `pointing` over the Mac's first pass, 6,332 tiles linked in: 4,877
   images, 309,518 registrations, residual median 1.83 px, 27 % over 5 px;
   8.6 s. Against Friday's fit, which had the 157 replaced tiles too: 0.025
   px apart at the median, 0.6 at the 99th percentile.
3. `second`, 30 big and 30 small tiles: heights 0.65 and 1.3 m RMS from
   Friday's at the median, correlation with their own images the same; one
   small tile Friday's run had wrong (35 m off, 2 images used) now 3 m and
   8 images.
4. `score`, the same held-out pairs as Friday: big tiles 0.577 against
   0.593, small 0.376 against 0.411 -- see the placement below.

## Where the second pass puts a tile

Friday's run moved each 241-point tile by the joint fit's offset, from the
plan's centre; the example does not. The same held-out pairs, tiles at the
plan's centre, moved by the fitted offset, moved as far in a random
direction:

| tiles | pairs | at the plan's centre | the fitted offset | a random one |
|---|---|---|---|---|
| 1000-1059 | 87 | 0.577 | 0.577 | 0.578 |
| 2000-2029 | 48 | 0.577 | 0.593 | 0.593 |

Nothing in the fitted offset: on one range no placement matters, on the
other any move of a few metres does as well as it. A tile's window slides
over SHAP7 without moving the relief, and which ground it takes in moves a
sample's score by a hundredth. The example keeps the plan's centre; its
held-out figures should come within that of Friday's 0.581.

## Open

- The final set of tiles for `compose` and `comet67p_shadows_fit`: the
  second pass's, the bad and the below-SHAP7 left out, in the new numbering
  (`tiles_e2e/` holds the first pass linked in under it).
- The Mac's tiles with a window cut (at most 1,065), with `fetch`'s lines
  now whole.
