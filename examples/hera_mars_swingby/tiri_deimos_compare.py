#!/usr/bin/env python
"""Observed TIRI frames beside the simulated ones, for the swing-by.

Answers three questions, in the order they have to be answered:

1. **Is Deimos in the same place?** Pure geometry -- pointing, ephemeris and
   frame conventions. If this is wrong nothing else matters.
2. **Is Mars where we say it is?** It is not rendered into the FITS, but its
   predicted disc is drawn over both panels so the limb can be checked.
3. **Do the pixel values agree?** Only in structure. The observed frames are
   the JAXA/VITO/ROB calibrated radiances, in the same W m^-2 sr^-1 the
   simulation carries, so they are compared on one shared scale. The header
   still says `BUNIT = DN`, which it inherits from the raw product; the values
   are radiance.
"""

from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy
import pandas
import spiceypy as spice
from astropy.io import fits
from matplotlib.patches import Circle

import kalast

KERNEL = "/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops_local.tm"
IMAGES = "/Users/gregoireh/data/hera/tiri/tiri_images_mars_swing-by_deimos.csv"
REAL = Path("/Users/gregoireh/data/hera/tiri/JAXA-VITO-ROB radiances comparison/"
            "Deimos radiances")
SIM = Path("out/hera_mars_swingby/tiri_deimos_fits")
OUT = Path("out/hera_mars_swingby/deimos_real_vs_sim.png")
R_MARS = 3396.2

spice.kclear()
spice.furnsh(KERNEL)
tiri = kalast.entity.TIRI
NPX, NPY = int(tiri.px[0]), int(tiri.px[1])
fovy = numpy.radians(tiri.fovy)
images = pandas.read_csv(IMAGES)


def project(v):
    """A point in the TIRI frame to pixel coordinates, or None if behind."""
    v = numpy.asarray(v, dtype=float)
    if v[2] <= 0:
        return None
    th = numpy.tan(fovy / 2.0)
    x = v[0] / v[2] / (th * NPX / NPY)
    y = v[1] / v[2] / th
    return (0.5 * (1.0 + x) * NPX, 0.5 * (1.0 - y) * NPY)


rows = []
for _, r in images.iterrows():
    et = float(r["et"])
    real = REAL / r["image"].replace("tiri_raw_", "tiri_rad_")
    sim = SIM / (Path(r["image"]).stem + f"_sim_{ {'Filter a (7.8um)':'a','Filter b (8.6um)':'b','Filter c (9.6um)':'c','Filter d (10.6um)':'d','Filter e (11.6um)':'e','Filter f (13.0um)':'f','Filter g (wide)':'g'}[r['filter']] }.fits")
    if not real.exists() or not sim.exists():
        print(f"  missing: {real.name if not real.exists() else sim.name}")
        continue
    rows.append((et, r, real, sim))
print(f"{len(rows)} pairs")

n = len(rows)
fig, axes = plt.subplots(n, 2, figsize=(9.5, 3.1 * n), facecolor="0.1")
for k, (et, r, realp, simp) in enumerate(rows):
    dreal = fits.getdata(realp).astype(float)
    dreal = numpy.where(numpy.isfinite(dreal), dreal, 0.0)
    dsim = fits.getdata(simp).astype(float)
    hsim = fits.getheader(simp)

    pd_, _ = spice.spkpos("DEIMOS", et, tiri.frame, "none", "HERA")
    pm, _ = spice.spkpos("MARS", et, tiri.frame, "none", "HERA")
    xy_d, xy_m = project(pd_), project(pm)
    mn = numpy.linalg.norm(pm)
    r_mars_px = numpy.degrees(numpy.arcsin(R_MARS / mn)) / tiri.fovy * NPY

    for j, (img, title, cmap) in enumerate((
            (dreal, f"OBSERVED  {realp.name}\ncalibrated radiance", "inferno"),
            (dsim, f"SIMULATED  {hsim['FW_NUM']}\nW m$^{{-2}}$ sr$^{{-1}}$", "inferno"))):
        ax = axes[k, j]
        finite = img[numpy.isfinite(img)]
        # One shared scale across both panels: the point is whether the values
        # agree, which a per-panel stretch would hide.
        lo, hi = 0.0, 26.0
        if not finite.size:
            hi = 1.0
        ax.imshow(img, cmap=cmap, vmin=lo, vmax=hi, origin="lower")
        # predicted positions, drawn on BOTH panels
        if xy_d:
            ax.plot(*xy_d, "+", color="lime", ms=13, mew=1.6)
            ax.add_patch(Circle(xy_d, 18, fill=False, ec="lime", lw=1.0, alpha=0.8))
        if xy_m:
            ax.add_patch(Circle(xy_m, r_mars_px, fill=False, ec="deepskyblue",
                                lw=1.2, ls="--", alpha=0.9))
        ax.set_xlim(0, NPX); ax.set_ylim(0, NPY)
        ax.set_title(title, color="w", fontsize=8)
        ax.set_xticks([]); ax.set_yticks([])
    axes[k, 0].text(0.01, 0.98, f"{hsim['DATE-OBS'][11:19]}   "
                    f"{hsim['RANGE']:.0f} km   {hsim['NPIXBODY']:,} px sim",
                    color="w", fontsize=7, va="top", transform=axes[k, 0].transAxes)
    if hsim["NPIXBODY"] == 0:
        axes[k, 1].text(0.5, 0.5, "Deimos predicted\noutside FOV", color="tomato",
                        ha="center", va="center", fontsize=10, weight="bold",
                        transform=axes[k, 1].transAxes)

fig.suptitle("TIRI Mars swing-by — observed vs simulated.  green + = predicted "
             "Deimos,  blue dashed = predicted Mars limb", color="w", fontsize=12)
fig.tight_layout(rect=[0, 0, 1, 0.985])
OUT.parent.mkdir(parents=True, exist_ok=True)
fig.savefig(OUT, dpi=100, facecolor="0.1")
print(f"wrote {OUT}")
