#!/usr/bin/env python
"""One Dimorphos orbit through the study epoch, as frames.

Context for the FITS delivered to GIS3D: those are a single instant, and a
single instant of a binary in mutual event is hard to read. This renders the
same scene, through the same TIRI pointing, across +/-6.5 h of the study
epoch -- 13 hours, slightly more than one 11.37 h Dimorphos orbit -- so the
shadow can be watched arriving, crossing and fading.

Three frames per epoch:

    diffuse      what the renderer draws, for geometric context
    temperature  surface temperature per pixel, via the facet-index buffer
    radiance     TIRI wide band, the quantity the FITS actually carry

Temperatures are the phase 2 snapshots, used at their own epochs rather than
interpolated, so every frame is a state the model actually computed. That
fixes the cadence at SNAP_EVERY * dt = 280 s and gives ~168 frames.

TIRI is nadir-pointed at Didymos throughout this window -- boresight measured
0.000 deg off-axis at every hour -- so the real pointing needs no
substitution.
"""

import time
from pathlib import Path

import numpy
import pandas
import spiceypy as spice
import matplotlib
matplotlib.use("Agg")
from matplotlib import cm, colors, image as mpimg

import kalast
import kalast.tpm.radiance as radiance

# ---------------------------------------------------------------- settings
EPOCH = "2027-01-21 05:36:00 UTC"
HALF_WINDOW_H = 6.5
PHASE2 = "out/hera_didymos/phase2_mutual"
OUT = Path("out/hera_didymos/tiri_movie")
FILTER = "g"

# Fixed scales, so a frame means the same thing as the one before it. An
# autoscaled sequence makes a cooling body look constant.
T_RANGE = (80.0, 370.0)
L_RANGE = (0.0, 60.0)
CMAP = "inferno"

RESPONSE = "/Users/gregoireh/data/hera/tiri/response.csv"
KERNEL = "/Users/gregoireh/data/spice/hera/kernels/mk/hera_plan_local.tm"
BODIES = ("DIDYMOS", "DIMORPHOS")
MESH = {
    "DIDYMOS": (
        "/Users/gregoireh/data/mesh/didymos/"
        "g_01165mm_spc_obj_didy_0000n00000_v003_decimated_10k.obj"
    ),
    "DIMORPHOS": (
        "/Users/gregoireh/data/mesh/dimorphos/"
        "g_00243mm_spc_obj_dimo_0000n00000_v004_decimated_10k.obj"
    ),
}
R_DIDY, R_DIMO = 0.390, 0.085

# ------------------------------------------------------------------ setup
spice.kclear()
spice.furnsh(KERNEL)
et_study = spice.str2et(EPOCH)

tiri = kalast.entity.TIRI
NPX, NPY = int(tiri.px[0]), int(tiri.px[1])
REF_FRAME = kalast.entity.DIDYMOS.frame

snap_et = numpy.load(f"{PHASE2}/snap_et.npy")
tsurf = {b: numpy.load(f"{PHASE2}/{b.lower()}/snap_tsurf.npy") for b in BODIES}

keep = numpy.abs(snap_et - et_study) <= HALF_WINDOW_H * 3600.0
frame_idx = numpy.where(keep)[0]
frame_et = snap_et[keep]
n_frames = frame_et.size

emissivity = float(kalast.tpm.properties.DIDYMOS.emissivity)
band = radiance.tiri_bands(RESPONSE, emissivity=emissivity)[FILTER]

print(f"TIRI context movie, {EPOCH} +/- {HALF_WINDOW_H} h")
print(f"  {n_frames} frames, cadence {numpy.diff(frame_et).mean():.1f} s, "
      f"{spice.et2utc(frame_et[0], 'C', 0)} -> {spice.et2utc(frame_et[-1], 'C', 0)}")
print(f"  Dimorphos orbit is {kalast.entity.DIMORPHOS.spin_period / 3600:.2f} h, "
      f"so this spans {2 * HALF_WINDOW_H / (kalast.entity.DIMORPHOS.spin_period / 3600):.2f} of it")

for d in ("diffuse", "temperature", "radiance"):
    (OUT / d).mkdir(parents=True, exist_ok=True)

# -------------------------------------------------------------- rendering
app = kalast.app.App()
app.config.width = NPX
app.config.height = NPY
app.config.vsync = False
app.config.export_dir = str(OUT / "diffuse")
app.config.access_shadow_map = False

app.simulation.camera.projection.fovy = numpy.radians(tiri.fovy)
for b in BODIES:
    app.simulation.load_mesh(path=MESH[b], mat=numpy.eye(4), flatten=True)
n_facets = [len(app.simulation.bodies[i].mesh.facets) for i in range(len(BODIES))]

t_norm = colors.Normalize(*T_RANGE)
l_norm = colors.Normalize(*L_RANGE)
cmap = cm.get_cmap(CMAP) if hasattr(cm, "get_cmap") else matplotlib.colormaps[CMAP]

rows = []
clock = {"t0": None, "done": False}


def place(sim, et):
    sim.bodies[0].mat[:3, :3] = numpy.eye(3)
    sim.bodies[0].mat[:3, 3] = [0.0, 0.0, 0.0]
    (p_dimo, _lt) = spice.spkpos("DIMORPHOS", et, REF_FRAME, "none", "DIDYMOS")
    sim.bodies[1].mat[:3, :3] = spice.pxform("DIMORPHOS_FIXED", REF_FRAME, et)
    sim.bodies[1].mat[:3, 3] = p_dimo

    (p_hera, _lt) = spice.spkpos("HERA", et, REF_FRAME, "lt+s", "DIDYMOS")
    m = spice.pxform(tiri.frame, REF_FRAME, et)
    sim.camera.pos = numpy.asarray(p_hera, dtype=numpy.float64)
    sim.camera.dir = m @ numpy.array([0.0, 0.0, 1.0])
    sim.camera.up = m @ numpy.array([1.0, 0.0, 0.0])

    (p_sun, _lt) = spice.spkpos("SUN", et, REF_FRAME, "none", "DIDYMOS")
    u = numpy.asarray(p_sun) / numpy.linalg.norm(p_sun)
    sim.sun.pos = u * 50.0
    sim.sun.look_anchor()
    return numpy.asarray(p_sun), numpy.asarray(p_dimo)


def before_render(sim, dt_frame):
    k = sim.state.iteration
    if k >= n_frames or clock["done"]:
        return
    place(sim, frame_et[k])
    sim.export_once()      # the diffuse frame
    sim.request_facet_id()  # the geometry behind the other two


def after_render(sim, dt_frame):
    k = sim.state.iteration
    if clock["done"]:
        return
    if clock["t0"] is None:
        clock["t0"] = time.perf_counter()

    if k >= n_frames:
        finish()
        return

    result = sim.facet_id_map()
    if result is None:
        return
    ids, offsets = numpy.asarray(result[0]), numpy.asarray(result[1])
    et = frame_et[k]
    i_snap = frame_idx[k]

    tmap = numpy.zeros(ids.shape, dtype=numpy.float64)
    filled = numpy.zeros(ids.shape, dtype=bool)
    for i, b in enumerate(BODIES):
        m = (ids > offsets[i]) & (ids <= offsets[i] + n_facets[i])
        tmap[m] = tsurf[b][i_snap][ids[m] - offsets[i] - 1]
        filled |= m

    # RGBA straight from the colormap, background left black. Written with
    # imsave rather than a figure: 168 matplotlib figures would dominate the
    # runtime, and a bare image is what a frame sequence wants anyway.
    rgba_t = cmap(t_norm(numpy.where(filled, tmap, numpy.nan)))
    rgba_t[~filled] = (0, 0, 0, 1)
    mpimg.imsave(OUT / "temperature" / f"{k:04d}.png", rgba_t)

    lmap = numpy.zeros(ids.shape)
    lmap[filled] = band(tmap[filled])
    rgba_l = cmap(l_norm(numpy.where(filled, lmap, numpy.nan)))
    rgba_l[~filled] = (0, 0, 0, 1)
    mpimg.imsave(OUT / "radiance" / f"{k:04d}.png", rgba_l)

    # Per-frame geometry, so the sequence can be read without re-deriving it.
    (p_sun, _lt) = spice.spkpos("SUN", et, REF_FRAME, "none", "DIDYMOS")
    (p_dimo, _lt) = spice.spkpos("DIMORPHOS", et, REF_FRAME, "none", "DIDYMOS")
    u = numpy.asarray(p_sun) / numpy.linalg.norm(p_sun)
    along = float(numpy.dot(p_dimo, u))
    perp = float(numpy.linalg.norm(p_dimo - along * u))
    sun_ang = 6.957e5 / numpy.linalg.norm(p_sun)
    ys, xs = numpy.where(filled)
    rows.append({
        "frame": k,
        "et": et,
        "utc": spice.et2utc(et, "C", 0),
        "hours_from_epoch": (et - et_study) / 3600.0,
        "eclipse_on_primary": along > 0 and perp < R_DIDY + R_DIMO + along * sun_ang,
        "secondary_in_umbra": along < 0 and perp < R_DIDY - abs(along) * sun_ang + R_DIMO,
        "t_max": float(tmap[filled].max()) if filled.any() else 0.0,
        "t_mean_lit": float(tmap[filled].mean()) if filled.any() else 0.0,
        "l_max": float(lmap[filled].max()) if filled.any() else 0.0,
        "px": int(filled.sum()),
        "row0": int(ys.min()), "row1": int(ys.max()),
        "col0": int(xs.min()), "col1": int(xs.max()),
    })

    if k % 20 == 0 or k == n_frames - 1:
        el = time.perf_counter() - clock["t0"]
        print(f"  {k + 1:4d}/{n_frames}  {rows[-1]['utc']}  "
              f"{rows[-1]['hours_from_epoch']:+6.2f} h  "
              f"Tmax {rows[-1]['t_max']:5.1f} K  "
              f"{'ECLIPSE' if rows[-1]['eclipse_on_primary'] else ''}"
              f"{'  UMBRA' if rows[-1]['secondary_in_umbra'] else ''}"
              f"   [{el:.0f}s]")


def finish():
    clock["done"] = True
    df = pandas.DataFrame(rows)
    df.to_csv(OUT / "frames.csv", index=False, encoding="utf-8-sig")
    el = time.perf_counter() - clock["t0"]
    print(f"\n{len(rows)} frames in {el:.1f}s")
    print(f"  eclipse on the primary in {int(df.eclipse_on_primary.sum())} frames, "
          f"secondary in umbra in {int(df.secondary_in_umbra.sum())}")
    print(f"  bodies span rows {df.row0.min()}-{df.row1.max()}, "
          f"cols {df.col0.min()}-{df.col1.max()} across the sequence")
    print(f"  temperature scale {T_RANGE} K, radiance scale {L_RANGE} W/m2/sr")
    print(f"wrote {OUT}/")
    import os
    os._exit(0)


app.before_render = before_render
app.after_render = after_render
app.start()
