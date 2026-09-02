#!/usr/bin/env python
"""The Mars swing-by approach as frames: Deimos, with Mars behind it.

Context for the FITS in `tiri_deimos_fits.py`. Those are seventeen instants
during thirteen minutes in which Deimos goes from 7,596 km to 549 km and grows
from 7 px to 101 px; a single frame does not convey that.

Three panels per epoch:

    diffuse      what the renderer draws, for geometric context
    temperature  surface temperature per pixel, via the facet-index buffer
    radiance     TIRI wide band, the quantity the FITS carry

**Mars is rendered in a second pass and composited behind.** Each body gets a
frustum fitted to itself rather than one global frustum fitted to the pair --
Mars is 3,396 km across at ~90,000 km, so a shared shadow frustum would leave
6 km Deimos with almost no shadow-map resolution and destroy its
self-shadowing. Compositing is the cheapest form of per-body frusta, and it is
exact here because Mars is measured to be *behind* Deimos at every epoch, so
the depth order never has to be resolved per pixel.

**Mars carries no thermophysical model.** kalast does not model it, so its
brightness is the instantaneous radiative-equilibrium temperature of a
non-conducting surface, `T = T_ss cos(i)^(1/4)`, which is the standard
zeroth-order approximation and is flagged as such. It is a placeholder for the
radiative-transfer output that will replace it; what it gets right is where
Mars is, how big, and roughly how the limb darkens.
"""

import time
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy
import pandas
import spiceypy as spice

import kalast
import kalast.tpm.nonuniform as nonuniform
import kalast.tpm.properties as properties
import kalast.tpm.radiance as radiance
import kalast.tpm.routine as routine
from kalast.util import AU, SOLAR_CONSTANT, STEFAN_BOLTZMANN

KERNEL = "/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops_local.tm"
MESH = "/Users/gregoireh/data/mesh/deimos/deimos_k005_tho_v02.obj"
SPHERE = "/Users/gregoireh/data/mesh/sphere4.obj"     # unit radius, 20,480 facets
RESPONSE = "/Users/gregoireh/data/hera/tiri/response.csv"
RESTART = "out/hera_mars_swingby/deimos_tpm"
OUT = Path("out/hera_mars_swingby/tiri_deimos_movie")

UTC0, UTC1 = "2025-03-12 11:50:00", "2025-03-12 12:12:00"
N_FRAMES = 120
R_MARS = 3396.2
T_SS_MARS = 260.0     # subsolar equilibrium at 1.66 AU, Mars albedo ~0.25
T_NIGHT_MARS = 150.0  # night-side floor; a placeholder, see mars_temperature
MARS_EMIS = 0.95
SCALE = 4             # render at 1/SCALE of the detector, for speed

spice.kclear()
spice.furnsh(KERNEL)
body = kalast.entity.DEIMOS
prop = kalast.tpm.properties.DEIMOS
prop.se = STEFAN_BOLTZMANN * prop.emissivity
prop.compute_conductivity_diffusivity()
D = prop.diffusivity
bands = radiance.tiri_bands(RESPONSE, emissivity=prop.emissivity)
band_g = bands["g"]
mars_band = radiance.BandRadiance(band_g.wavelength, band_g.response,
                                  emissivity=MARS_EMIS)

z = nonuniform.column(
    properties.skin_depth_1(D, body.spin_period), m=4, n=5,
    b=properties.skin_depth_2pi(D, kalast.entity.MARS.orbit_period)
    / properties.skin_depth_1(D, body.spin_period))
T = pandas.read_csv(Path(RESTART) / "tmp_state.csv").to_numpy()

tiri = kalast.entity.TIRI
NPX, NPY = int(tiri.px[0]) // SCALE, int(tiri.px[1]) // SCALE
et0, et1 = spice.str2et(UTC0 + " UTC"), spice.str2et(UTC1 + " UTC")
ets = numpy.linspace(et0, et1, N_FRAMES)

print(f"Deimos swing-by context movie")
print(f"  {UTC0} -> {UTC1}, {N_FRAMES} frames at {NPX}x{NPY}")
OUT.mkdir(parents=True, exist_ok=True)

app = kalast.app.App()
app.config.width = NPX
app.config.height = NPY
app.config.vsync = False
app.config.access_shadow_map = True
app.simulation.camera.projection.fovy = numpy.radians(tiri.fovy)

# Two bodies loaded, but only one is ever *placed in front of the camera* at a
# time: Deimos is drawn with the scene fitted to Deimos, Mars with the scene
# fitted to Mars, and the two images composited. Loading both up front avoids
# rebuilding GPU buffers between passes.
app.simulation.load_mesh(path=MESH, mat=numpy.eye(4), flatten=True)
app.simulation.load_mesh(path=SPHERE, mat=numpy.eye(4), flatten=True)
deimos_mesh = app.simulation.bodies[0].mesh
nface = len(deimos_mesh.facets)
mars_mesh = app.simulation.bodies[1].mesh
n_mars = len(mars_mesh.facets)
mars_normals = numpy.array([mars_mesh.facets[k].normal for k in range(n_mars)])

FAR = 1.0e9   # km; both passes park the unused body far behind the camera
state = {"i": 0, "phase": 0, "deimos": None, "written": 0}
t0 = time.perf_counter()


def before_render(sim, _dt):
    i = state["i"]
    if i >= N_FRAMES:
        return
    et = ets[i]
    ps, _lt = spice.spkpos("SUN", et, tiri.frame, "none", "HERA")
    u_sun = numpy.asarray(ps) / numpy.linalg.norm(ps)

    pd_, _lt = spice.spkpos("DEIMOS", et, tiri.frame, "none", "HERA")
    pm, _lt = spice.spkpos("MARS", et, tiri.frame, "none", "HERA")

    if state["phase"] == 0:
        # Deimos pass: Mars pushed out of the scene entirely, so scene_bounds
        # -- and therefore the shadow frustum -- fits Deimos alone.
        sim.bodies[0].mat[:3, :3] = spice.pxform(body.frame, tiri.frame, et)
        sim.bodies[0].mat[:3, 3] = pd_
        sim.bodies[1].mat[:3, :3] = numpy.eye(3) * 1.0e-6
        sim.bodies[1].mat[:3, 3] = [0.0, 0.0, -FAR]
        sim.sun.anchor = list(numpy.asarray(pd_, dtype=float))
        sim.sun.pos = numpy.asarray(pd_) + u_sun * 200.0
        sim.sun.look_anchor()
        sim.request_facet_id()
    else:
        # Mars pass: Deimos out, Mars in, sphere scaled to its radius.
        sim.bodies[0].mat[:3, :3] = numpy.eye(3) * 1.0e-6
        sim.bodies[0].mat[:3, 3] = [0.0, 0.0, -FAR]
        sim.bodies[1].mat[:3, :3] = numpy.eye(3) * R_MARS
        sim.bodies[1].mat[:3, 3] = pm
        sim.sun.anchor = list(numpy.asarray(pm, dtype=float))
        sim.sun.pos = numpy.asarray(pm) + u_sun * 2.0e5
        sim.sun.look_anchor()
        sim.request_facet_id()

    sim.camera.pos = [0.0, 0.0, 0.0]
    sim.camera.dir = [0.0, 0.0, 1.0]
    sim.camera.up = [0.0, -1.0, 0.0]
    sim.hud = f"{i}/{N_FRAMES}  {'deimos' if state['phase']==0 else 'mars  '}"


def mars_temperature(ids, offsets, et, pm):
    """Instantaneous equilibrium, `T_ss cos(i)^1/4`. No TPM: see the docstring."""
    # The facet-index buffer is one space across all loaded bodies:
    # `1 + offsets[b] + facet`. Mars is body 1, so its ids start above
    # Deimos's 5,040 and the offset has to come off before indexing.
    lo = int(offsets[1])
    filled = (ids > lo) & (ids <= lo + n_mars)
    out = numpy.zeros(ids.shape)
    if not filled.any():
        return out, filled
    f = numpy.where(filled, ids - 1 - lo, 0)
    ps, _lt = spice.spkpos("SUN", et, tiri.frame, "none", "HERA")
    # Sun direction from Mars's centre, in the camera frame; the sphere's
    # normals are its own outward radials, unrotated.
    u = numpy.asarray(ps) - numpy.asarray(pm)
    u = u / numpy.linalg.norm(u)
    cosi = numpy.clip(mars_normals[f[filled]] @ u, 0.0, None)
    # Floored at the table's lower limit. `cos(i)^1/4` sends the terminator to
    # a few kelvin, which is both unphysical -- a real atmosphere-bearing Mars
    # has a night side near 150 K, not 8 K -- and outside the tabulated band
    # range. The floor is a placeholder standing in for the night-side
    # temperature the radiative-transfer model will supply.
    out[filled] = numpy.maximum(
        T_SS_MARS * numpy.power(numpy.maximum(cosi, 1e-6), 0.25), T_NIGHT_MARS)
    return out, filled


def after_render(sim, _dt):
    i = state["i"]
    if i >= N_FRAMES:
        return
    et = ets[i]
    got = sim.facet_id_map()
    if got is None:
        return
    ids = numpy.asarray(got[0])

    offsets = numpy.asarray(got[1])
    if state["phase"] == 0:
        lo = int(offsets[0])
        filled = (ids > lo) & (ids <= lo + nface)
        f = numpy.where(filled, ids - 1 - lo, 0)
        tmap = numpy.zeros(ids.shape)
        tmap[filled] = T[f[filled], 0]
        state["deimos"] = (tmap, filled)
        state["phase"] = 1
        return

    pm, _lt = spice.spkpos("MARS", et, tiri.frame, "none", "HERA")
    tmars, mfilled = mars_temperature(ids, offsets, et, pm)
    tdeimos, dfilled = state["deimos"]

    # Composite: Deimos in front, Mars behind, space elsewhere. Exact here --
    # Mars is measured to be farther than Deimos at every epoch.
    tmap = numpy.where(dfilled, tdeimos, numpy.where(mfilled, tmars, 0.0))
    rad = numpy.zeros(tmap.shape)
    rad[dfilled] = band_g(tdeimos[dfilled])
    only_mars = mfilled & ~dfilled
    rad[only_mars] = mars_band(tmars[only_mars])

    fig, axes = plt.subplots(1, 3, figsize=(13.5, 3.9), facecolor="0.1")
    for ax, img, title, cmap, vmax in (
            (axes[0], (tmap > 0).astype(float) + 0.6 * (dfilled), "geometry",
             "bone", 1.6),
            (axes[1], tmap, "surface temperature [K]", "magma", 300.0),
            (axes[2], rad, "TIRI wide-band radiance [W m$^{-2}$ sr$^{-1}$]",
             "inferno", 22.0)):
        im = ax.imshow(img, cmap=cmap, vmin=0, vmax=vmax, origin="lower")
        ax.set_title(title, color="w", fontsize=9)
        ax.set_xticks([]); ax.set_yticks([])
        if title != "geometry":
            cb = fig.colorbar(im, ax=ax, fraction=0.03)
            cb.ax.tick_params(colors="w", labelsize=7)
    d = numpy.linalg.norm(spice.spkpos("DEIMOS", et, tiri.frame, "none", "HERA")[0])
    fig.suptitle(f"{spice.et2utc(et,'ISOC',0)}   Deimos {d:,.0f} km   "
                 f"{int(dfilled.sum()):,} px   Mars {int(only_mars.sum()):,} px",
                 color="w", fontsize=11)
    fig.tight_layout(rect=[0, 0, 1, 0.93])
    fig.savefig(OUT / f"{i:04d}.png", dpi=95, facecolor="0.1")
    plt.close(fig)

    state["written"] += 1
    if i % 20 == 0:
        el = time.perf_counter() - t0
        print(f"  {i:3d}/{N_FRAMES}  Deimos {d:8,.0f} km  "
              f"{int(dfilled.sum()):6,} px   Mars {int(only_mars.sum()):7,} px"
              f"   {el:5.1f}s", flush=True)

    state["phase"] = 0
    state["i"] += 1
    if state["i"] >= N_FRAMES:
        print(f"\nwrote {state['written']} frames to {OUT}/")
        import os, sys
        sys.stdout.flush()
        os._exit(0)


app.before_render = before_render
app.after_render = after_render
app.start()
