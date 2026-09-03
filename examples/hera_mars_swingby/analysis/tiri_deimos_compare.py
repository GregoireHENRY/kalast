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
from scipy.ndimage import gaussian_filter
from astropy.io import fits
from matplotlib.patches import Circle, Patch
from matplotlib.lines import Line2D

import kalast
import kalast.tiri_alignment as tiri_align  # 0.60 deg alignment the FK lacks

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
    """TIRI-frame vector to array index (column, row).

    Both axes are negated: the detector is 180 deg from a naive +X-right,
    +Y-down frame. Fixed against the real calibrated radiances, where
    Deimos's measured column runs opposite to the un-negated prediction
    (residual 270 px vs 17 px), and where only this convention reprojects
    Mars to a lat/lon map that is consistent between epochs."""
    v = numpy.asarray(v, dtype=float)
    if v[2] <= 0:
        return None
    th = numpy.tan(fovy / 2.0)
    return (0.5 * (1.0 - v[0] / v[2] / (th * NPX / NPY)) * NPX,
            0.5 * (1.0 - v[1] / v[2] / th) * NPY)



def _rays():
    """Unit ray per pixel, inverting `project`."""
    th = numpy.tan(fovy / 2.0)
    col, row = numpy.meshgrid(numpy.arange(NPX) + 0.5, numpy.arange(NPY) + 0.5)
    d = numpy.stack([-(2.0 * col / NPX - 1.0) * th * NPX / NPY,
                     -(2.0 * row / NPY - 1.0) * th,
                     numpy.ones_like(col)], axis=-1)
    return d / numpy.linalg.norm(d, axis=-1, keepdims=True)


_RAY = _rays()


def mars_diffuse(et):
    """Grey diffuse Mars for the simulated panel.

    The simulated FITS carry Deimos radiance only -- Mars is not thermally
    modelled -- so without this the simulated column is black wherever the
    observed one shows Mars, and the two cannot be compared by eye. Mars is a
    sphere, so it is intersected analytically rather than rendered. Grey, and
    on its own scale: nothing here is radiometric.
    """
    c = tiri_align.apply(spice.spkpos("MARS", et, tiri.frame, "none", "HERA")[0])
    su = tiri_align.apply(spice.spkpos("SUN", et, tiri.frame, "none", "HERA")[0])
    cn = float(numpy.linalg.norm(c))
    img = numpy.zeros((NPY, NPX))
    if cn <= R_MARS:
        return img
    b = _RAY @ c
    disc = b ** 2 - (cn ** 2 - R_MARS ** 2)
    hit = (disc > 0.0) & (b > 0.0)
    if hit.any():
        t = b[hit] - numpy.sqrt(disc[hit])
        n = (_RAY[hit] * t[:, None] - c) / R_MARS
        u = (su - c) / numpy.linalg.norm(su - c)
        img[hit] = numpy.clip(n @ u, 0.0, None)
    return img


def detect(img):
    """Centroid of the brightest *compact* source, or None.

    The previous version thresholded at median + 20 MAD over the whole frame
    and kept the largest cluster. Both halves fail once Mars is in shot:

    - at 12:07:06 Mars is partly in frame, passes the threshold, and wins the
      cluster-size test outright, putting the marker on its limb 656 px from
      Deimos;
    - from 12:07:48 Mars *fills* the frame, so the median and MAD become Mars's
      own statistics, the threshold rises above Deimos, and nothing is
      detected at all.

    High-passing first -- a narrow Gaussian minus a wide one -- flattens Mars's
    smooth limb-to-limb gradient and leaves structure on Deimos's scale, which
    is the thing being looked for. Size is then irrelevant, so an extended body
    cannot win by being big.
    """
    o = numpy.where(numpy.isfinite(img), img, 0.0)
    hp = gaussian_filter(o, 1.2) - gaussian_filter(o, 12.0)
    sd = float(hp.std())
    if not numpy.isfinite(sd) or sd <= 0.0:
        return None
    j, i = numpy.unravel_index(hp.argmax(), hp.shape)
    if hp[j, i] < 6.0 * sd:
        return None
    w = 7
    sl = numpy.s_[max(0, j - w):j + w + 1, max(0, i - w):i + w + 1]
    sub = hp[sl].copy()
    sub[sub < 0.0] = 0.0
    if sub.sum() <= 0.0:
        return None
    yy, xx = numpy.mgrid[sl]
    return (float((sub * xx).sum() / sub.sum()),
            float((sub * yy).sum() / sub.sum()))


rows = []
for _, r in images.iterrows():
    real = REAL / r["image"].replace("tiri_raw_", "tiri_rad_")
    sim = SIM / (Path(r["image"]).stem + f"_sim_{KEY[r['filter']]}.fits")
    if real.exists() and sim.exists():
        rows.append((float(r["et"]), r, real, sim))
print(f"{len(rows)} pairs")

n = len(rows)
# A fixed header in inches, not a fraction: the figure is ~3.15 in per row, so a
# fractional reserve that looks right for 3 rows leaves nothing for 17 and the
# title lands on top of the first panel.
HEADER_IN = 1.75
fig, axes = plt.subplots(n, 2, figsize=(9.6, 3.15 * n + HEADER_IN),
                         facecolor="0.1")
offsets = []
for k, (et, r, realp, simp) in enumerate(rows):
    dreal = fits.getdata(realp).astype(float)
    dreal = numpy.where(numpy.isfinite(dreal), dreal, 0.0)
    dsim = fits.getdata(simp).astype(float)
    hsim = fits.getheader(simp)

    pd_, _ = spice.spkpos("DEIMOS", et, tiri.frame, "none", "HERA")
    pm, _ = spice.spkpos("MARS", et, tiri.frame, "none", "HERA")
    # Same rotation the simulated frames were rendered with, so the overlay and
    # the image it is drawn on agree. See kalast/tiri_alignment.py.
    pd_ = tiri_align.apply(pd_)
    pm = tiri_align.apply(pm)
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
        if j == 1:
            # Mars underneath, grey and non-radiometric, so this panel shows the
            # same scene the observed one does.
            ax.imshow(mars_diffuse(et), cmap="gray", vmin=0.0, vmax=1.15,
                      origin="lower")
            img = numpy.ma.masked_where(img <= 0.0, img)
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

fig_h = 3.15 * n + HEADER_IN
fig.tight_layout(rect=[0, 0, 1, 1.0 - HEADER_IN / fig_h])

fig.text(0.5, 1.0 - 0.40 / fig_h,
         "HERA TIRI, Mars swing-by 2025-03-12 - observed vs kalast simulation",
         color="w", fontsize=13, ha="center", va="center")
fig.text(0.5, 1.0 - 0.70 / fig_h,
         "left: real calibrated radiance.   right: simulated - Deimos radiance "
         "on the same scale, Mars shown grey for context only (not radiometric).",
         color="0.72", fontsize=8.5, ha="center", va="center")

# A real legend with the actual markers, rather than the marker names spelled
# out in the title.
handles = [
    Line2D([], [], ls="none", marker="+", color="lime", ms=12, mew=1.8,
           label="Deimos predicted (kalast)"),
    Line2D([], [], ls="none", marker="x", color="red", ms=9, mew=1.8,
           label="Deimos measured (real frame)"),
    Line2D([], [], ls="--", color="deepskyblue", lw=1.4,
           label="Mars limb predicted"),
    Patch(facecolor="0.55", edgecolor="none",
          label="Mars diffuse - context only"),
]
# Two columns, not four: at 960 px four rows of text ran off both edges.
fig.legend(handles=handles, loc="center", ncol=2,
           bbox_to_anchor=(0.5, 1.0 - 1.24 / fig_h),
           frameon=False, fontsize=9, labelcolor="w",
           handletextpad=0.7, columnspacing=3.0, labelspacing=0.5)
OUT.parent.mkdir(parents=True, exist_ok=True)
fig.savefig(OUT, dpi=100, facecolor="0.1")
print(f"wrote {OUT}")
print("\npredicted - observed, px:")
for t, dx, dy in offsets:
    print(f"  {t}  ({dx:+7.1f},{dy:+7.1f})   {numpy.hypot(dx,dy):6.1f} px  "
          f"{numpy.hypot(dx,dy)/NPY*tiri.fovy:5.2f} deg")
