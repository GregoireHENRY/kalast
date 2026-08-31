#!/usr/bin/env python
"""Simulated TIRI radiance images of the Didymos system, written as FITS.

The deliverable for GIS3D: what TIRI would record at 2027-01-21T05:36 UTC,
the mid-point of Dimorphos's shadow transit across Didymos, with pixel values
in physical radiance rather than DN.

How a pixel gets its value
--------------------------

Not by reading back the colour image. Colour quantises to 8 bits, is
entangled with lighting and tone mapping, and recovering a physical quantity
from it means inverting a colormap. Instead the scene is rendered a second
time into an integer target holding the *facet index* per pixel
(`sim.request_facet_id`), and the radiance is then looked up per facet in
numpy at full float precision:

    ids   -> facet index per pixel, 0 where nothing was drawn
    T     -> surface temperature per facet, from the phase 2 TPM
    L(T)  -> band-averaged spectral radiance for the chosen filter

Visibility comes free with it. The ID pass carries its own depth buffer, so
occlusion is resolved by the rasteriser: a facet missing from the map is one
TIRI genuinely cannot see -- behind the limb, outside the field of view, or
hidden by the other body.

No emission-angle cosine
------------------------

A grey Lambertian surface emits the same *radiance* in every direction; the
cosine enters only when integrating radiance over a surface to get flux. A
pixel measures radiance, and the projected area is already accounted for by
which pixels the facet covers. So the pixel value is `eps * B_band(T)` with
no angular factor. (`rad.py` carries a `cose` term because it goes on to sum
irradiance over the disk, which is a different quantity.)

Temperatures
------------

From `tpm_phase2.py`, whose snapshots are sampled every 5 steps. The state at
the study epoch is taken from those rather than from the run's final state,
which is three Didymos spins later. Section 7.5b measures why that matters:
the eclipse scar is -93.7 K at the epoch and -10 K one rotation later, so
which instant is used changes the image completely.
"""

from pathlib import Path

import numpy
import spiceypy as spice
from astropy.io import fits

import kalast
import kalast.tpm.radiance as radiance

# ---------------------------------------------------------------- settings
EPOCH = "2027-01-21 05:36:00 UTC"
PHASE2 = "out/hera_didymos/phase2_mutual"
OUT = Path("out/hera_didymos/tiri_fits")
FILTERS = radiance.TIRI_FILTERS  # a-g; g is the wide band

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

# ------------------------------------------------------------------ setup
spice.kclear()
spice.furnsh(KERNEL)
et = spice.str2et(EPOCH)

tiri = kalast.entity.TIRI
NPX, NPY = int(tiri.px[0]), int(tiri.px[1])
didymos = kalast.entity.DIDYMOS
REF_FRAME = didymos.frame

# Surface temperature at the study epoch, from the phase 2 snapshots.
# `tsurf_at_epoch.npy` is the step landing nearest the study epoch, saved
# exactly rather than picked from the coarse snapshot series.
temperature = {
    b: numpy.load(f"{PHASE2}/{b.lower()}/tsurf_at_epoch.npy") for b in BODIES
}

emissivity = float(kalast.tpm.properties.DIDYMOS.emissivity)
bands = radiance.tiri_bands(RESPONSE, emissivity=emissivity)

print(f"TIRI simulated radiance, {EPOCH}")
print(f"  detector {NPX}x{NPY}, fovy {tiri.fovy:.2f} deg")
print(f"  temperatures from {PHASE2}, at the study epoch")
for b in BODIES:
    t = temperature[b]
    print(f"  {b:10s} {t.size:,} facets, {t.min():.1f}-{t.max():.1f} K")

# -------------------------------------------------------------- rendering
app = kalast.app.App()
app.config.width = NPX
app.config.height = NPY
app.config.vsync = False
# The ID pass reads geometry, not the shadow map, so nothing here needs it.
app.config.access_shadow_map = False

app.simulation.camera.projection.fovy = numpy.radians(tiri.fovy)

for b in BODIES:
    app.simulation.load_mesh(path=MESH[b], mat=numpy.eye(4), flatten=True)

n_facets = [len(app.simulation.bodies[i].mesh.facets) for i in range(len(BODIES))]
for b, n in zip(BODIES, n_facets):
    if n != temperature[b].size:
        raise SystemExit(
            f"{b}: mesh has {n:,} facets but the TPM state has "
            f"{temperature[b].size:,} -- they must be the same mesh"
        )

done = {"v": False}


def before_render(sim, dt_frame):
    if done["v"]:
        return

    # Scene in Didymos's body-fixed frame, matching tpm_phase2.py so the
    # facet indices mean the same thing in both.
    sim.bodies[0].mat[:3, :3] = numpy.eye(3)
    sim.bodies[0].mat[:3, 3] = [0.0, 0.0, 0.0]
    (p_dimo, _lt) = spice.spkpos("DIMORPHOS", et, REF_FRAME, "none", "DIDYMOS")
    sim.bodies[1].mat[:3, :3] = spice.pxform("DIMORPHOS_FIXED", REF_FRAME, et)
    sim.bodies[1].mat[:3, 3] = p_dimo

    # TIRI's own pointing: boresight along the instrument frame's +Z, and the
    # reference (up) vector along +X -- the convention the Cosmographia
    # cross-check used.
    (p_hera, _lt) = spice.spkpos("HERA", et, REF_FRAME, "lt+s", "DIDYMOS")
    m = spice.pxform(tiri.frame, REF_FRAME, et)
    sim.camera.pos = numpy.asarray(p_hera, dtype=numpy.float64)
    sim.camera.dir = m @ numpy.array([0.0, 0.0, 1.0])
    sim.camera.up = m @ numpy.array([1.0, 0.0, 0.0])

    # The Sun only lights the colour image, which is not the product; set it
    # anyway so a visual check of the frame looks right.
    (p_sun, _lt) = spice.spkpos("SUN", et, REF_FRAME, "none", "DIDYMOS")
    sim.sun.pos = numpy.asarray(p_sun) / numpy.linalg.norm(p_sun) * 50.0
    sim.sun.look_anchor()

    sim.request_facet_id()


def after_render(sim, dt_frame):
    if done["v"]:
        return
    result = sim.facet_id_map()
    if result is None:
        return
    done["v"] = True

    ids, offsets = result
    ids = numpy.asarray(ids)
    offsets = numpy.asarray(offsets)
    write(ids, offsets, sim)
    import os
    os._exit(0)


def write(ids, offsets, sim):
    OUT.mkdir(parents=True, exist_ok=True)

    # Decode the global index into (body, facet). ids hold 1 + offset + facet,
    # with 0 reserved for "nothing drawn".
    body_of = numpy.full(ids.shape, -1, dtype=numpy.int32)
    facet_of = numpy.zeros(ids.shape, dtype=numpy.int64)
    for i, (b, off) in enumerate(zip(BODIES, offsets)):
        lo, hi = off, off + n_facets[i]
        m = (ids > lo) & (ids <= hi)
        body_of[m] = i
        facet_of[m] = ids[m] - lo - 1

    filled = body_of >= 0
    print(f"\n{filled.sum():,} of {ids.size:,} pixels on a body "
          f"({100 * filled.mean():.2f}% fill)")
    for i, b in enumerate(BODIES):
        m = body_of == i
        seen = numpy.unique(facet_of[m]).size if m.any() else 0
        print(f"  {b:10s} {int(m.sum()):>8,} px, {seen:,} distinct facets visible "
              f"of {n_facets[i]:,}")

    # Per-pixel temperature, then radiance per filter.
    tmap = numpy.zeros(ids.shape, dtype=numpy.float64)
    for i, b in enumerate(BODIES):
        m = body_of == i
        tmap[m] = temperature[b][facet_of[m]]

    (p_hera, _lt) = spice.spkpos("HERA", et, REF_FRAME, "lt+s", "DIDYMOS")
    range_km = float(numpy.linalg.norm(p_hera))

    for f in FILTERS:
        band = bands[f]
        img = numpy.zeros(ids.shape, dtype=numpy.float32)
        img[filled] = band(tmap[filled]).astype(numpy.float32)

        hdu = fits.PrimaryHDU(img)
        h = hdu.header
        h["BUNIT"] = ("W/m2/sr/um", "band-averaged spectral radiance")
        h["INSTRUME"] = ("TIRI", "Thermal InfraRed Imager")
        h["TELESCOP"] = ("HERA", "ESA Hera mission")
        h["ORIGIN"] = ("kalast", "simulated, not observed")
        h["DATE-OBS"] = (spice.et2utc(et, "ISOC", 3), "simulated epoch, UTC")
        h["FILTER"] = (f, "TIRI filter, g is the wide band")
        h["WAVELEN"] = (round(band.effective_wavelength * 1e6, 4),
                        "[um] response-weighted effective wavelength")
        h["EMISSIV"] = (emissivity, "grey emissivity assumed")
        h["RANGE"] = (round(range_km, 4), "[km] Hera to Didymos, lt+s")
        h["FOVY"] = (tiri.fovy, "[deg] detector vertical field of view")

        # Enough of the model to reproduce the number in any pixel.
        h["TPMGRID"] = ("geometric-nonuniform", "depth grid")
        h["TPMSTEN"] = ("variable-spacing", "conduction stencil")
        h["TPMSOLV"] = ("explicit-forward-euler", "time integration")
        h["TPMSPIN"] = (3, "solar orbits of spin-up, direct insolation only")
        h["TPMSEG"] = ("6 spins", "high-fidelity segment before the epoch")
        h["SHADOW"] = ("mutual", "self-shadowing + mutual eclipse, shadow map")
        h["HEATING"] = ("none", "mutual and self heating NOT included")
        h["THERMINE"] = (float(kalast.tpm.properties.DIDYMOS.thermal_inertia),
                         "[J/m2/K/s^0.5] thermal inertia")
        h["ALBEDO"] = (float(kalast.tpm.properties.DIDYMOS.albedo), "bond albedo")
        h["MESHFAC"] = (int(sum(n_facets)), "total facets, both bodies")
        h["RADMETH"] = ("facet-id buffer", "per-pixel facet index, no colormap")
        h["EMISCOS"] = (False, "no emission-angle cosine: Lambertian radiance")
        h.add_comment("Simulated TIRI image produced by kalast.")
        h.add_comment("Pixel value is band-averaged spectral radiance,")
        h.add_comment("normalised by the integral of the filter response, so")
        h.add_comment("the arbitrary scale of the response cancels.")
        h.add_comment("Zero marks pixels with no body in them.")

        path = OUT / f"tiri_didymos_{f}.fits"
        hdu.writeto(path, overwrite=True)
        lit = img[filled]
        print(f"  filter {f} ({band.effective_wavelength * 1e6:5.2f} um): "
              f"{lit.min():7.4f} - {lit.max():7.4f} W/m2/sr/um  -> {path.name}")

    numpy.save(OUT / "facet_ids.npy", ids)
    numpy.save(OUT / "temperature_map.npy", tmap)
    print(f"\nwrote {OUT}/")


app.before_render = before_render
app.after_render = after_render
app.start()
