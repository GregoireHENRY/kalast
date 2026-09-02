"""A viewable PNG for every simulated Deimos FITS: Mars diffuse, Deimos radiance.

The FITS carry Deimos radiance only, because Mars is not thermally modelled --
putting a fabricated Mars temperature in a radiance product would be wrong. For
looking at, though, Mars is exactly what gives the frame its context, so here it
is added as a **diffuse-lit sphere in grey** underneath, with Deimos's real
modelled radiance in `inferno` on top and the geometric limb drawn over both.
Grey is deliberate: nothing in the Mars layer is radiometric.

Mars is intersected analytically rather than rendered. It is a sphere, so each
pixel's ray either meets it or does not, and the limb is the exact cone of
half-angle asin(R/d) about the direction to Mars -- no render, no resolution
limit on the outline, and it runs in seconds for all 17 frames.

The projection is the one the frames were rendered with: both detector axes
negated (see kalast/tiri_alignment.py), and the same 0.60 deg alignment applied
to every TIRI-frame vector, so the limb lands where the render put Mars.

Run:  python examples/hera_mars_swingby/tiri_deimos_png.py
"""

from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.colors import LinearSegmentedColormap
import numpy
import spiceypy as spice
from astropy.io import fits

import kalast
import kalast.tiri_alignment as tiri_align

KERNEL = "/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops_local.tm"
SIM = Path("out/hera_mars_swingby/tiri_deimos_fits")
OUT = Path("out/hera_mars_swingby/tiri_deimos_png")
R_MARS = 3396.2
VMAX = 26.0                      # W m^-2 sr^-1, common scale across the set

spice.kclear()
spice.furnsh(KERNEL)
tiri = kalast.entity.TIRI
NPX, NPY = int(tiri.px[0]), int(tiri.px[1])
fovy = numpy.radians(tiri.fovy)
TH = numpy.tan(fovy / 2.0)
THX = TH * NPX / NPY
OUT.mkdir(parents=True, exist_ok=True)


def rays():
    """Unit ray per pixel, inverting the render's projection."""
    col, row = numpy.meshgrid(numpy.arange(NPX) + 0.5, numpy.arange(NPY) + 0.5)
    x = -(2.0 * col / NPX - 1.0) * THX
    y = -(2.0 * row / NPY - 1.0) * TH
    d = numpy.stack([x, y, numpy.ones_like(x)], axis=-1)
    return d / numpy.linalg.norm(d, axis=-1, keepdims=True)


RAY = rays()


def mars_diffuse(et):
    """Grey diffuse Mars, and the pixels it covers. Analytic sphere."""
    c = tiri_align.apply(spice.spkpos("MARS", et, tiri.frame, "none", "HERA")[0])
    s = tiri_align.apply(spice.spkpos("SUN", et, tiri.frame, "none", "HERA")[0])
    cn = float(numpy.linalg.norm(c))
    if cn <= R_MARS:
        return numpy.zeros((NPY, NPX)), numpy.zeros((NPY, NPX), bool), c, cn
    b = RAY @ c
    disc = b ** 2 - (cn ** 2 - R_MARS ** 2)
    hit = (disc > 0.0) & (b > 0.0)
    img = numpy.zeros((NPY, NPX))
    if hit.any():
        t = b[hit] - numpy.sqrt(disc[hit])
        p = RAY[hit] * t[:, None]
        n = (p - c) / R_MARS
        u = (s - c) / numpy.linalg.norm(s - c)
        img[hit] = numpy.clip(n @ u, 0.0, None)
    return img, hit, c, cn


def limb(c, cn):
    """The exact apparent limb: a cone of half-angle asin(R/d) about c."""
    w = c / cn
    a = numpy.array([0.0, 0.0, 1.0])
    if abs(w @ a) > 0.9:
        a = numpy.array([1.0, 0.0, 0.0])
    e1 = numpy.cross(w, a); e1 /= numpy.linalg.norm(e1)
    e2 = numpy.cross(w, e1)
    al = numpy.arcsin(R_MARS / cn)
    th = numpy.linspace(0.0, 2.0 * numpy.pi, 720)
    v = (numpy.cos(al) * w[None, :]
         + numpy.sin(al) * (numpy.cos(th)[:, None] * e1[None, :]
                            + numpy.sin(th)[:, None] * e2[None, :]))
    ok = v[:, 2] > 0.0
    if not ok.any():
        return None, None
    v = v[ok]
    return (0.5 * (1.0 - v[:, 0] / v[:, 2] / THX) * NPX,
            0.5 * (1.0 - v[:, 1] / v[:, 2] / TH) * NPY)


files = sorted(SIM.glob("*_sim_*.fits"))
print(f"{len(files)} simulated frames -> {OUT}/\n")
for f in files:
    rad = numpy.asarray(fits.getdata(f), float)
    h = fits.getheader(f)
    et = float(h["ET"])
    mars, hit, c, cn = mars_diffuse(et)
    lit = rad > 0.0

    fig, ax = plt.subplots(figsize=(10.24, 7.68), dpi=100, facecolor="0.08")
    ax.set_position([0, 0, 1, 1])
    # Mars first, in grey: context, not radiometry.
    ax.imshow(mars, cmap="gray", vmin=0.0, vmax=1.15, origin="lower",
              interpolation="nearest")
    # Deimos on top, in its own radiance scale.
    m = numpy.ma.masked_where(~lit, rad)
    im = ax.imshow(m, cmap="inferno", vmin=0.0, vmax=VMAX, origin="lower",
                   interpolation="nearest")
    lx, ly = limb(c, cn)
    if lx is not None:
        ax.plot(lx, ly, "-", color="#35d0ff", lw=1.1, alpha=0.9)
    ax.set_xlim(0, NPX); ax.set_ylim(0, NPY)
    ax.set_xticks([]); ax.set_yticks([])

    npix = int(lit.sum())
    ttl = (f"{h['DATE-OBS'][:19]}   filter {h['FW_NUM']}   "
           f"Deimos {h['RANGE']:.0f} km, {h['GSD']:.0f} m/px, {npix:,} px")
    if npix == 0:
        ttl += "   [Deimos outside the field]"
    ax.text(0.012, 0.978, ttl, transform=ax.transAxes, color="w", fontsize=10,
            va="top", family="monospace")
    ax.text(0.012, 0.022,
            "Deimos: modelled radiance (inferno).  Mars: diffuse lighting only, "
            "not radiometric (grey).  Cyan: geometric Mars limb.",
            transform=ax.transAxes, color="0.75", fontsize=8, va="bottom")
    cax = fig.add_axes([0.86, 0.10, 0.015, 0.30])
    cb = fig.colorbar(im, cax=cax)
    cb.set_label("Deimos radiance  W m$^{-2}$ sr$^{-1}$", color="w", fontsize=8)
    cb.ax.tick_params(colors="w", labelsize=7)
    cb.outline.set_edgecolor("0.5")

    name = f.stem + ".png"
    fig.savefig(OUT / name, facecolor="0.08")
    plt.close(fig)
    print(f"  {name}   Deimos {npix:6,} px   Mars {int(hit.sum()):7,} px"
          f"   limb {'in frame' if lx is not None else 'behind/absent'}")
print(f"\nwrote {len(files)} PNGs to {OUT}/")
