#!/usr/bin/env python
"""Crop and stack the three frame sequences into one strip per epoch.

`tiri_movie.py` writes full 1024x768 TIRI frames, in which the pair occupies
1.6% of the area. Useful as a faithful product, unreadable as a movie. This
crops every frame to one bounding box -- the same box for all of them, taken
from the whole sequence, so nothing drifts or rescales between frames -- and
stacks diffuse | temperature | radiance side by side with a caption.

Kept separate from the export so the frames can be recomposed (different
crop, different pairing) without re-rendering.
"""

from pathlib import Path

import numpy
import pandas
import matplotlib
matplotlib.use("Agg")
from matplotlib import cm, colors, pyplot
from mpl_toolkits.axes_grid1 import make_axes_locatable

IN = Path("out/hera_didymos/tiri_movie")
OUT = IN / "strip"
PAD = 14
T_RANGE = (80.0, 370.0)
L_RANGE = (0.0, 60.0)

df = pandas.read_csv(IN / "frames.csv")
OUT.mkdir(parents=True, exist_ok=True)

# One box for the whole sequence: a per-frame box would make the bodies
# appear to sit still while the frame moved around them.
r0 = max(int(df.row0.min()) - PAD, 0)
r1 = int(df.row1.max()) + PAD
c0 = max(int(df.col0.min()) - PAD, 0)
c1 = int(df.col1.max()) + PAD
print(f"common crop rows {r0}-{r1}, cols {c0}-{c1} "
      f"({r1 - r0}x{c1 - c0}) from {len(df)} frames")

cmap = matplotlib.colormaps["inferno"]
for _, row in df.iterrows():
    k = int(row.frame)
    # The renderer names its exports by its own counter, zero-padded, which
    # starts at 0 and increments per exported frame -- the same order as ours.
    imgs = [
        matplotlib.image.imread(IN / "diffuse" / f"{k:06d}.png"),
        matplotlib.image.imread(IN / "temperature" / f"{k:04d}.png"),
        matplotlib.image.imread(IN / "radiance" / f"{k:04d}.png"),
    ]
    fig, ax = pyplot.subplots(1, 3, figsize=(13.5, 4.4))
    titles = ["diffuse (geometry)", "surface temperature [K]",
              "TIRI wide band [W/m2/sr]"]
    ranges = [None, T_RANGE, L_RANGE]

    # `fig.colorbar(ax=a)` takes its space *out of* `a`, so a panel with a
    # colorbar ends up drawn smaller than one without. That made the three
    # bodies different sizes across the strip even though the underlying
    # frames are pixel-identical -- verified at 12,092 px and the same
    # bounding box in both. Appending a fixed-width axes to every panel,
    # including an invisible one for the diffuse frame, keeps all three image
    # axes the same size so the panels are directly comparable.
    for a, im, t, rng in zip(ax, imgs, titles, ranges):
        a.imshow(im[r0:r1, c0:c1])
        a.set_title(t, fontsize=10)
        a.set_xticks([]); a.set_yticks([])

        cax = make_axes_locatable(a).append_axes("right", size="5%", pad=0.08)
        if rng is None:
            cax.axis("off")
        else:
            fig.colorbar(cm.ScalarMappable(colors.Normalize(*rng), cmap),
                         cax=cax)

    tags = []
    if row.eclipse_on_primary:
        tags.append("Dimorphos shadow on Didymos")
    if row.secondary_in_umbra:
        tags.append("Dimorphos in Didymos umbra")
    fig.suptitle(
        f"{row.utc}   T{row.hours_from_epoch:+.2f} h from study epoch"
        + ("   |   " + "  +  ".join(tags) if tags else ""),
        fontsize=11,
    )
    fig.tight_layout()
    fig.savefig(OUT / f"{k:04d}.png", dpi=100)
    pyplot.close(fig)

print(f"wrote {len(df)} strips to {OUT}/")
print("to make a movie:")
print(f"  ffmpeg -framerate 12 -i {OUT}/%04d.png -c:v libx264 "
      f"-pix_fmt yuv420p {IN}/tiri_context.mp4")
