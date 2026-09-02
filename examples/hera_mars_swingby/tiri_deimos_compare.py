#!/usr/bin/env python
"""Observed TIRI frames beside the simulated ones, pixel for pixel.

Both panels are drawn identically -- same orientation, same shared radiance
scale -- so a difference on the page is a difference in the data.

Three markers, and the distinction between them is the point:

    green +   where the ephemeris and the pointing kernel put Deimos
    red x     where Deimos actually is, centroided from the observed frame
    blue - -  where the kernels put Mars's limb

**They do not coincide.** On the three frames where Mars is out of shot and
Deimos is unambiguous, the observed body sits about (+40, -39) px from the
prediction -- a **0.7 degree pointing offset**, steady across the sequence.
That is not an orientation error in this code: the rendered body lands within
0.3 px of the same projection, checked directly. It is a disagreement between
the predicted attitude and where TIRI was actually looking.

The observed frames are the JAXA/VITO/ROB calibrated radiances, in the same
W m^-2 sr^-1 the simulation carries; the header still reads `BUNIT = DN`,
inherited from the raw product.
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
VMAX = 26.0

spice.kclear()
spice.furnsh(KERNEL)
tiri = kalast.entity.TIRI
NPX, NPY = int(tiri.px[0]), int(tiri.px[1])
fovy = numpy.radians(tiri.fovy)
images = pandas.read_csv(IMAGES)
KEY = {"Filter a (7.8um)": "a", "Filter b (8.6um)": "b", "Filter c (9.6um)": "c",
       "Filter d (10.6um)": "d", "Filter e (11.6um)": "e", "Filter f (13.0um)": "f",
       "Filter g (wide)": "g"}


def project(v):
    """TIRI-frame vector to pixel. No Y inversion: the panels use
    `origin="lower"`, so +Y maps to increasing row. Verified against the
    render, which lands within 0.3 px of this."""
    v = numpy.asarray(v, dtype=float)
    if v[2] <= 0:
        return None
    th = numpy.tan(fovy / 2.0)
    return (0.5 * (1.0 + v[0] / v[2] / (th * NPX / NPY)) * NPX,
            0.5 * (1.0 + v[1] / v[2] / th) * NPY)


def detect(img):
    """Centroid of the brightest compact source, or None."""
    o = numpy.where(numpy.isfinite(img), img, numpy.nan)
    bg = numpy.nanmedian(o)
    mad = numpy.nanmedian(numpy.abs(o - bg))
    if not numpy.isfinite(mad) or mad <= 0:
        return None
    m = numpy.isfinite(o) & (o > bg + 20 * 1.4826 * mad)
    if m.sum() < 3:
        return None
    ys, xs = numpy.nonzero(m)
    pts = numpy.column_stack([xs, ys])
    used = numpy.zeros(len(pts), bool)
    best = None
    for i in range(len(pts)):
        if used[i]:
            continue
        d = numpy.hypot(pts[:, 0] - pts[i, 0], pts[:, 1] - pts[i, 1])
        g = (d < 30) & ~used
        used |= g
        if best is None or g.sum() > len(best):
            best = pts[g]
    if best is None or len(best) < 3:
        return None
    w = o[best[:, 1], best[:, 0]] - bg
    if w.sum() <= 0:
        return None
    return ((best[:, 0] * w).sum() / w.sum(), (best[:, 1] * w).sum() / w.sum())


rows = []
for _, r in images.iterrows():
    real = REAL / r["image"].replace("tiri_raw_", "tiri_rad_")
    sim = SIM / (Path(r["image"]).stem + f"_sim_{KEY[r['filter']]}.fits")
    if real.exists() and sim.exists():
        rows.append((float(r["et"]), r, real, sim))
print(f"{len(rows)} pairs")

n = len(rows)
fig, axes = plt.subplots(n, 2, figsize=(9.6, 3.15 * n), facecolor="0.1")
offsets = []
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
    obs_c = detect(dreal)
    if xy_d and obs_c:
        offsets.append((hsim["DATE-OBS"][11:19], xy_d[0] - obs_c[0], xy_d[1] - obs_c[1]))

    for j, (img, title, cmap) in enumerate((
            (dreal, f"OBSERVED  {realp.name}\ncalibrated radiance", "inferno"),
            (dsim, f"SIMULATED  {hsim['FW_NUM']}\nW m$^{{-2}}$ sr$^{{-1}}$", "inferno"))):
        ax = axes[k, j]
        ax.imshow(img, cmap=cmap, vmin=0.0, vmax=VMAX, origin="lower")
        if xy_d:
            ax.plot(*xy_d, "+", color="lime", ms=14, mew=1.7)
        if obs_c:
            ax.plot(*obs_c, "x", color="red", ms=11, mew=1.7)
        if xy_m:
            ax.add_patch(Circle(xy_m, r_mars_px, fill=False, ec="deepskyblue",
                                lw=1.2, ls="--", alpha=0.9))
        ax.set_xlim(0, NPX)
        ax.set_ylim(0, NPY)
        ax.set_title(title, color="w", fontsize=8)
        ax.set_xticks([])
        ax.set_yticks([])
    lab = (f"{hsim['DATE-OBS'][11:19]}   {hsim['RANGE']:.0f} km   "
           f"{hsim['NPIXBODY']:,} px simulated")
    if xy_d and obs_c:
        lab += f"\npredicted - observed = ({xy_d[0]-obs_c[0]:+.0f},{xy_d[1]-obs_c[1]:+.0f}) px"
    axes[k, 0].text(0.01, 0.98, lab, color="w", fontsize=7, va="top",
                    transform=axes[k, 0].transAxes)
    if hsim["NPIXBODY"] == 0:
        axes[k, 1].text(0.5, 0.5, "Deimos predicted\noutside FOV", color="tomato",
                        ha="center", va="center", fontsize=10, weight="bold",
                        transform=axes[k, 1].transAxes)

fig.suptitle("TIRI Mars swing-by, observed vs simulated.   green + predicted "
             "Deimos    red x observed Deimos    blue - - predicted Mars limb",
             color="w", fontsize=11)
fig.tight_layout(rect=[0, 0, 1, 0.987])
OUT.parent.mkdir(parents=True, exist_ok=True)
fig.savefig(OUT, dpi=100, facecolor="0.1")
print(f"wrote {OUT}")
print("\npredicted - observed, px:")
for t, dx, dy in offsets:
    print(f"  {t}  ({dx:+7.1f},{dy:+7.1f})   {numpy.hypot(dx,dy):6.1f} px  "
          f"{numpy.hypot(dx,dy)/NPY*tiri.fovy:5.2f} deg")
