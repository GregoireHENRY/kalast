#!/usr/bin/env python
"""One frame rendered exactly as TIRI would see it, for a like-for-like check.

12:08:36 is the diagnostic frame: filter g, Deimos crossing Mars's limb with
the planet behind it, so both bodies constrain the geometry at once. Rendered
at the full 1024x768 with the instrument's own FOV from the IK, in the same
orientation as the FITS, so it can be laid directly against the observed frame
and against Cosmographia or ShapeViewer.

Mars is diffuse geometry only -- kalast has no thermophysical model for it.
Two passes, one per body, each with the shadow frustum fitted to itself, then
composited: a shared frustum sized by Mars at 3,396 km would leave 6 km Deimos
with almost no shadow-map resolution.
"""

import sys
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy
import pandas
import spiceypy as spice
from astropy.io import fits

import kalast
import kalast.tpm.nonuniform as nonuniform
import kalast.tpm.properties as properties
import kalast.tpm.radiance as radiance
import kalast.tpm.routine as routine
from kalast.util import AU, SOLAR_CONSTANT, STEFAN_BOLTZMANN

ET = 795053385.2                       # 2025-03-12T12:08:36
KERNEL = "/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops_local.tm"
MESH = "/Users/gregoireh/data/mesh/deimos/deimos_k005_tho_v02.obj"
SPHERE = "/Users/gregoireh/data/mesh/sphere4.obj"
RESTART = "out/hera_mars_swingby/deimos_tpm"
OBS = Path("/Users/gregoireh/data/hera/tiri/JAXA-VITO-ROB radiances comparison/"
           "Deimos radiances/tiri_rad_20250312_120836_31_0.fit")
OUT = Path("out/hera_mars_swingby/frame_120836")
R_MARS = 3396.2
PREROLL_ROT, DT_FINE = 2.0, 10.0

spice.kclear()
spice.furnsh(KERNEL)
body = kalast.entity.DEIMOS
prop = kalast.tpm.properties.DEIMOS
prop.se = STEFAN_BOLTZMANN * prop.emissivity
prop.compute_conductivity_diffusivity()
D = prop.diffusivity
g = radiance.tiri_bands("/Users/gregoireh/data/hera/tiri/response.csv",
                        emissivity=prop.emissivity)["g"]
z = nonuniform.column(
    properties.skin_depth_1(D, body.spin_period), m=4, n=5,
    b=properties.skin_depth_2pi(D, kalast.entity.MARS.orbit_period)
    / properties.skin_depth_1(D, body.spin_period))
twodz = 2.0 * (z[1] - z[0])
T = pandas.read_csv(Path(RESTART) / "tmp_state.csv").to_numpy()

tiri = kalast.entity.TIRI
NPX, NPY = int(tiri.px[0]), int(tiri.px[1])
# The instrument's own field, not the round number: getfov gives 6.65 x 5.00,
# against 6.667 x 5.0 from fovy and the aspect ratio. Worth 2.6 px at the edge.
_, _, _, _, bounds = spice.getfov(-91200, 10)
b0 = numpy.asarray(bounds[0])
HX = numpy.arctan2(abs(b0[0]), b0[2])
HY = numpy.arctan2(abs(b0[1]), b0[2])
print(f"TIRI FOV from the IK: {numpy.degrees(HX):.4f} x {numpy.degrees(HY):.4f} deg "
      f"half-angles, {NPX}x{NPY}")

app = kalast.app.App()
app.config.width = NPX
app.config.height = NPY
app.config.vsync = False
app.config.access_shadow_map = True
app.simulation.camera.projection.fovy = 2.0 * HY
app.simulation.load_mesh(path=MESH, mat=numpy.eye(4), flatten=True)
app.simulation.load_mesh(path=SPHERE, mat=numpy.eye(4), flatten=True)
nface = len(app.simulation.bodies[0].mesh.facets)
mars_mesh = app.simulation.bodies[1].mesh
n_mars = len(mars_mesh.facets)
mars_n = numpy.array([mars_mesh.facets[k].normal for k in range(n_mars)])
pos = numpy.array([app.simulation.bodies[0].mesh.facets[k].pos
                   for k in range(nface)]) * 1e3
nrm = numpy.array([app.simulation.bodies[0].mesh.facets[k].normal
                   for k in range(nface)])
coefs_f = tuple(numpy.asarray(c, numpy.float64)
                for c in routine.nonuniform_coefficients(z, DT_FINE))
d_nodes = numpy.full(z.size, D)
n_pre = int(PREROLL_ROT * body.spin_period / DT_FINE)
FAR = 1.0e9
st = {"i": 0, "phase": 0, "deimos": None}
OUT.mkdir(parents=True, exist_ok=True)


def place(sim, et, phase):
    pd_, _ = spice.spkpos("DEIMOS", et, tiri.frame, "none", "HERA")
    pm, _ = spice.spkpos("MARS", et, tiri.frame, "none", "HERA")
    ps, _ = spice.spkpos("SUN", et, tiri.frame, "none", "HERA")
    u = numpy.asarray(ps) / numpy.linalg.norm(ps)
    if phase == 0:
        sim.bodies[0].mat[:3, :3] = spice.pxform(body.frame, tiri.frame, et)
        sim.bodies[0].mat[:3, 3] = pd_
        sim.bodies[1].mat[:3, :3] = numpy.eye(3) * 1.0e-6
        sim.bodies[1].mat[:3, 3] = [0.0, 0.0, -FAR]
        anchor = pd_
        dist = 200.0
    else:
        sim.bodies[0].mat[:3, :3] = numpy.eye(3) * 1.0e-6
        sim.bodies[0].mat[:3, 3] = [0.0, 0.0, -FAR]
        sim.bodies[1].mat[:3, :3] = numpy.eye(3) * R_MARS
        sim.bodies[1].mat[:3, 3] = pm
        anchor = pm
        dist = 2.0e5
    sim.sun.anchor = list(numpy.asarray(anchor, dtype=float))
    sim.sun.pos = numpy.asarray(anchor) + u * dist
    sim.sun.look_anchor()
    sim.camera.pos = [0.0, 0.0, 0.0]
    sim.camera.dir = [0.0, 0.0, 1.0]
    sim.camera.up = [0.0, -1.0, 0.0]


def before_render(sim, _dt):
    i = st["i"]
    if i > n_pre:
        return
    et = ET - (n_pre - i) * DT_FINE
    place(sim, et if i < n_pre else ET, st["phase"] if i == n_pre else 0)
    if i == n_pre:
        sim.request_facet_id()
    sim.hud = f"{i}/{n_pre}"


def after_render(sim, _dt):
    i = st["i"]
    if i > n_pre:
        return
    if i == n_pre:
        got = sim.facet_id_map()
        if got is None:
            return
        ids = numpy.asarray(got[0])
        offs = numpy.asarray(got[1])
        if st["phase"] == 0:
            lo = int(offs[0])
            f = (ids > lo) & (ids <= lo + nface)
            tm = numpy.zeros(ids.shape)
            tm[f] = T[numpy.where(f, ids - 1 - lo, 0)[f], 0]
            st["deimos"] = (tm, f)
            st["phase"] = 1
            return
        lo = int(offs[1])
        mf = (ids > lo) & (ids <= lo + n_mars)
        tm, df = st["deimos"]
        pm, _ = spice.spkpos("MARS", ET, tiri.frame, "none", "HERA")
        ps, _ = spice.spkpos("SUN", ET, tiri.frame, "none", "HERA")
        us = numpy.asarray(ps) - numpy.asarray(pm)
        us /= numpy.linalg.norm(us)
        diffuse = numpy.zeros(ids.shape)
        if mf.any():
            fm = numpy.where(mf, ids - 1 - lo, 0)
            diffuse[mf] = numpy.clip(mars_n[fm[mf]] @ us, 0.0, None)
        rad = numpy.zeros(ids.shape)
        rad[df] = g(tm[df])

        fits.PrimaryHDU(rad.astype(numpy.float32)).writeto(
            OUT / "kalast_radiance.fits", overwrite=True)
        fits.PrimaryHDU(diffuse.astype(numpy.float32)).writeto(
            OUT / "kalast_mars_diffuse.fits", overwrite=True)
        obs = numpy.where(numpy.isfinite(fits.getdata(OBS)), fits.getdata(OBS), 0.0)

        fig, ax = plt.subplots(1, 3, figsize=(16.5, 4.4), facecolor="0.1")
        for a, img, t, cm, vm in (
                (ax[0], obs, "OBSERVED TIRI, calibrated radiance", "inferno", 26.0),
                (ax[1], rad + 0.0, "kalast: Deimos radiance", "inferno", 26.0),
                (ax[2], diffuse + 1.6 * df, "kalast: geometry (Mars diffuse + Deimos)",
                 "gray", 1.6)):
            a.imshow(img, cmap=cm, vmin=0, vmax=vm, origin="lower")
            a.set_title(t, color="w", fontsize=9)
            a.set_xticks([]); a.set_yticks([])
        fig.suptitle("2025-03-12T12:08:36  filter g  Deimos 977 km, crossing Mars",
                     color="w", fontsize=12)
        fig.tight_layout(rect=[0, 0, 1, 0.94])
        fig.savefig(OUT / "three_way.png", dpi=110, facecolor="0.1")
        print(f"  Deimos {int(df.sum()):,} px, Mars {int((mf & ~df).sum()):,} px")
        print(f"  wrote {OUT}/")
        sys.stdout.flush()
        import os
        os._exit(0)

    et = ET - (n_pre - i) * DT_FINE
    ps, _ = spice.spkpos("SUN", et, body.frame, "none", "DEIMOS")
    ps = numpy.asarray(ps) * 1e3
    v = ps[None, :] - pos
    ds = numpy.linalg.norm(v, axis=1)
    cosi = numpy.einsum("ij,ij->i", nrm, v / ds[:, None])
    numpy.maximum(cosi, 0.0, out=cosi)
    fr = sim.facet_shadow(0)
    lit = 1.0 - numpy.asarray(fr, float) if fr is not None else numpy.ones(nface)
    routine.step_surface_newton(
        T, SOLAR_CONSTANT * (1 - prop.albedo) * cosi * lit / (ds / AU) ** 2,
        prop.se, prop.conductivity, twodz, threshold=0.1)
    routine.step_conduction(T, d_nodes, coefs_f)
    st["i"] += 1


app.before_render = before_render
app.after_render = after_render
app.start()
