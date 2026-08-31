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

Units
-----

`BUNIT = 'W m^-2 sr^-1'`, band-integrated radiance, matching what real
calibrated TIRI products carry. An earlier version wrote band-*averaged*
spectral radiance in W/m2/sr/um; that is nearly filter-independent, so the
wide band `g` came out no brighter than the narrow `a`, which is how the
mistake was caught. In the correct units `g` reads ~5x higher, its response
integral being 2.71 um against `a`'s 0.51 um.

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
from kalast.util import AU

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
R_DIDY, R_DIMO = 0.390, 0.085  # km, mean radii, for the event geometry

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


def geometry():
    """Everything about the scene worth recording in the header."""
    g = {}
    (p_hera_d, _lt) = spice.spkpos("HERA", et, REF_FRAME, "lt+s", "DIDYMOS")
    (p_hera_m, _lt) = spice.spkpos("HERA", et, "DIMORPHOS_FIXED", "lt+s", "DIMORPHOS")
    g["range_didymos"] = float(numpy.linalg.norm(p_hera_d))
    g["range_dimorphos"] = float(numpy.linalg.norm(p_hera_m))

    (p_sun, _lt) = spice.spkpos("SUN", et, REF_FRAME, "none", "DIDYMOS")
    g["sun_range_au"] = float(numpy.linalg.norm(p_sun)) / (AU / 1e3)
    g["phase"] = float(numpy.degrees(spice.vsep(p_sun, p_hera_d)))

    # Mutual geometry, in the shared heliocentric sense. `along > 0` puts
    # Dimorphos between the Sun and Didymos, so its shadow can reach the
    # primary; `along < 0` puts it behind, where the primary's umbra can
    # reach it.
    (p_dimo, _lt) = spice.spkpos("DIMORPHOS", et, REF_FRAME, "none", "DIDYMOS")
    u = numpy.asarray(p_sun) / numpy.linalg.norm(p_sun)
    along = float(numpy.dot(p_dimo, u))
    perp = float(numpy.linalg.norm(p_dimo - along * u))
    sun_ang = 6.957e5 / numpy.linalg.norm(p_sun)  # solar angular radius
    spot = R_DIMO + abs(along) * sun_ang          # penumbral spot radius
    umbra = R_DIDY - abs(along) * sun_ang         # umbra radius at the secondary
    g["sep"] = float(numpy.linalg.norm(p_dimo))
    g["eclipse_primary"] = bool(along > 0 and perp < R_DIDY + spot)
    g["eclipse_secondary"] = bool(along < 0 and perp < umbra + R_DIMO)
    g["eclipse_total_sec"] = bool(along < 0 and perp < umbra - R_DIMO)

    # Occultation is a line-of-sight question, not a lighting one: does the
    # secondary project onto the primary's disk as seen from Hera?
    (v_d, _lt) = spice.spkpos("DIDYMOS", et, "J2000", "lt+s", "HERA")
    (v_m, _lt) = spice.spkpos("DIMORPHOS", et, "J2000", "lt+s", "HERA")
    sep_ang = float(spice.vsep(v_d, v_m))
    ang_d = float(numpy.arcsin(R_DIDY / numpy.linalg.norm(v_d)))
    ang_m = float(numpy.arcsin(R_DIMO / numpy.linalg.norm(v_m)))
    g["ang_sep_arcsec"] = numpy.degrees(sep_ang) * 3600.0
    g["ang_radius_didymos"] = numpy.degrees(ang_d) * 3600.0
    g["ang_radius_dimorphos"] = numpy.degrees(ang_m) * 3600.0
    g["dimorphos_in_front"] = bool(
        numpy.linalg.norm(v_m) < numpy.linalg.norm(v_d))
    g["occultation"] = bool(sep_ang < ang_d + ang_m and g["dimorphos_in_front"])
    return g


def project_in_fov(world, sim):
    """Which of these world points fall inside the camera frustum.

    Built from the camera's own pos/dir/up and fovy rather than read back
    from the renderer, so it answers the geometric question independently of
    what the rasteriser happened to sample.
    """
    pos = numpy.asarray(sim.camera.pos, dtype=numpy.float64)
    fwd = numpy.asarray(sim.camera.dir, dtype=numpy.float64)
    fwd /= numpy.linalg.norm(fwd)
    up = numpy.asarray(sim.camera.up, dtype=numpy.float64)
    right = numpy.cross(fwd, up)
    right /= numpy.linalg.norm(right)
    true_up = numpy.cross(right, fwd)

    v = world - pos
    z = v @ fwd
    x = v @ right
    y = v @ true_up

    tan_half = numpy.tan(numpy.radians(tiri.fovy) / 2.0)
    aspect = NPX / NPY
    ahead = z > 1e-9
    zz = numpy.where(ahead, z, 1.0)
    return ahead & (numpy.abs(y / zz) <= tan_half) & (
        numpy.abs(x / zz) <= tan_half * aspect)


def latlon(position):
    """Body-fixed planetocentric latitude and east longitude, degrees."""
    r, lon, lat = spice.reclat(numpy.asarray(position, dtype=numpy.float64))
    return numpy.degrees(lat), numpy.degrees(lon) % 360.0, r


def write(ids, offsets, sim):
    OUT.mkdir(parents=True, exist_ok=True)

    # Decode the global index into (body, facet). ids hold 1 + offset + facet,
    # with 0 reserved for "nothing drawn".
    body_of = numpy.full(ids.shape, -1, dtype=numpy.int32)
    facet_of = numpy.zeros(ids.shape, dtype=numpy.int64)
    for i, (b, off) in enumerate(zip(BODIES, offsets)):
        m = (ids > off) & (ids <= off + n_facets[i])
        body_of[m] = i
        facet_of[m] = ids[m] - off - 1

    filled = body_of >= 0
    print(f"\n{filled.sum():,} of {ids.size:,} pixels on a body "
          f"({100 * filled.mean():.2f}% fill)")

    g = geometry()

    # Two different questions, easy to conflate:
    #
    #   in FOV      -- geometric. Does the facet project inside the frame?
    #                  This is what "is the body fully imaged" means.
    #   resolved    -- sampling. Did at least one pixel land on it?
    #
    # At 25.8 km Didymos spans ~133 px but carries 5,889 camera-facing
    # facets, so most are sub-pixel and go unsampled even though the body is
    # entirely in frame. Reporting the sampled fraction as coverage would
    # call a fully-imaged body "80% visible", which is a statement about mesh
    # resolution, not about the field of view.
    #
    # Both are area-weighted; counting facets would weight a sliver at the
    # limb the same as a facet at the sub-spacecraft point.
    coverage = {}
    for i, b in enumerate(BODIES):
        mesh = sim.bodies[i].mesh
        area = numpy.array([f.area for f in mesh.facets], dtype=numpy.float64)
        normal = numpy.array([f.normal for f in mesh.facets], dtype=numpy.float64)
        pos = numpy.array([f.pos for f in mesh.facets], dtype=numpy.float64)
        mat = numpy.asarray(sim.bodies[i].mat, dtype=numpy.float64)
        world = pos @ mat[:3, :3].T + mat[:3, 3]
        n_world = normal @ mat[:3, :3].T
        to_cam = numpy.asarray(sim.camera.pos, dtype=numpy.float64) - world
        to_cam /= numpy.linalg.norm(to_cam, axis=1)[:, None]
        facing = numpy.einsum("ij,ij->i", n_world, to_cam) > 0

        seen = numpy.zeros(n_facets[i], dtype=bool)
        m = body_of == i
        if m.any():
            seen[numpy.unique(facet_of[m])] = True

        in_fov = project_in_fov(world, sim)
        facing_area = max(area[facing].sum(), 1e-30)

        px = int(m.sum())
        coverage[b] = {
            "px": px,
            "seen": int(seen.sum()),
            "facing": int(facing.sum()),
            "fov_frac": float(area[facing & in_fov].sum() / facing_area),
            "res_frac": float(area[seen].sum() / facing_area),
            "total_area_frac": float(area[seen].sum() / area.sum()),
            "complete": bool(px > 0 and (facing & ~in_fov).sum() == 0),
        }
        c = coverage[b]
        print(f"  {b:10s} {c['px']:>8,} px | in FOV {100 * c['fov_frac']:5.1f}% of the "
              f"camera-facing area ({'complete' if c['complete'] else 'CLIPPED'}) | "
              f"resolved {100 * c['res_frac']:5.1f}% "
              f"({c['seen']:,}/{c['facing']:,} facets)")

    # Boresight intercept: the centre pixel already answers it, because the
    # camera looks along the instrument's +Z and the ID map is rendered
    # through that same view matrix.
    cy, cx = ids.shape[0] // 2, ids.shape[1] // 2
    bi = int(body_of[cy, cx])
    if bi >= 0:
        f_idx = int(facet_of[cy, cx])
        lat, lon, _r = latlon(sim.bodies[bi].mesh.facets[f_idx].pos)
        intercept = {"body": BODIES[bi], "facet": f_idx, "lat": lat, "lon": lon,
                     "t": float(temperature[BODIES[bi]][f_idx])}
        print(f"  boresight hits {BODIES[bi]} facet {f_idx} at "
              f"lat {lat:+.2f} lon {lon:.2f}, {intercept['t']:.1f} K")
    else:
        intercept = None
        print("  boresight hits no body (pointing off the pair)")

    print(f"  events: eclipse on primary={g['eclipse_primary']}, "
          f"secondary in umbra={g['eclipse_secondary']} "
          f"(total={g['eclipse_total_sec']}), occultation={g['occultation']}")

    # Per-pixel temperature, then radiance per filter.
    tmap = numpy.zeros(ids.shape, dtype=numpy.float64)
    for i, b in enumerate(BODIES):
        m = body_of == i
        tmap[m] = temperature[b][facet_of[m]]

    meta_kernel = Path(KERNEL).stem
    n_kernels = spice.ktotal("ALL")

    for f in FILTERS:
        band = bands[f]
        img = numpy.zeros(ids.shape, dtype=numpy.float32)
        img[filled] = band(tmap[filled]).astype(numpy.float32)

        hdu = fits.PrimaryHDU(img)
        h = hdu.header
        # --- identity, mirroring the real calibrated products -------------
        h["ORIGIN"] = ("kalast simulation", "SIMULATED, not observed")
        h["TELESCOP"] = ("HERA", "telescope used to acquire data")
        h["SPCECRFT"] = ("HERA", "name of spacecraft")
        h["INSTRUME"] = ("TIRI", "HERA Thermal Infrared Imager")
        h["BUNIT"] = ("W m^-2 sr^-1", "unit of pixel values")
        h["FW_NUM"] = (f"Filter {f}" + (" (wide)" if f == "g" else ""),
                       "filter wheel position")
        h["WAVELEN"] = (round(band.effective_wavelength * 1e6, 4),
                        "[um] response-weighted effective wavelength")
        h["BANDWID"] = (round(band.bandwidth, 4), "[um] integral of the response")
        h["EMISSIV"] = (emissivity, "grey emissivity assumed")

        # --- time and SPICE ----------------------------------------------
        h["SPI_TIME"] = (spice.et2utc(et, "ISOC", 4), "epoch, UTC")
        h["SPI_ET"] = (round(float(et), 4), "[s] SPICE ephemeris time (TDB)")
        h["SPI_MK"] = (meta_kernel, "SPICE metakernel")
        h["SPI_NK"] = (n_kernels, "SPICE kernels loaded")
        h["SPI_FRM"] = (REF_FRAME, "frame the scene is built in")

        # --- observing geometry -------------------------------------------
        h["RANGE_D"] = (round(g["range_didymos"], 4), "[km] Hera to Didymos, lt+s")
        h["RANGE_M"] = (round(g["range_dimorphos"], 4), "[km] Hera to Dimorphos")
        h["SEP_KM"] = (round(g["sep"], 5), "[km] Didymos to Dimorphos")
        h["SUNRANGE"] = (round(g["sun_range_au"], 6), "[AU] heliocentric distance")
        h["PHASE"] = (round(g["phase"], 4), "[deg] Sun-Didymos-Hera phase angle")
        h["FOVY"] = (tiri.fovy, "[deg] detector vertical field of view")
        h["ANGSEP"] = (round(g["ang_sep_arcsec"], 2),
                       "[arcsec] apparent separation of the two bodies")
        h["ANGRAD_D"] = (round(g["ang_radius_didymos"], 2),
                         "[arcsec] apparent radius of Didymos")
        h["ANGRAD_M"] = (round(g["ang_radius_dimorphos"], 2),
                         "[arcsec] apparent radius of Dimorphos")

        # --- boresight ------------------------------------------------------
        h["BSGHIT"] = (intercept is not None, "boresight intercepts a body")
        if intercept:
            h["BSGBODY"] = (intercept["body"], "body at the boresight")
            h["BSGFACET"] = (intercept["facet"], "facet index at the boresight")
            h["BSGLAT"] = (round(intercept["lat"], 4),
                           "[deg] body-fixed latitude at the boresight")
            h["BSGLON"] = (round(intercept["lon"], 4),
                           "[deg] body-fixed east longitude at the boresight")
            h["BSGTEMP"] = (round(intercept["t"], 3),
                            "[K] surface temperature at the boresight")

        # --- events ---------------------------------------------------------
        h["ECLIPS_P"] = (g["eclipse_primary"],
                         "Dimorphos shadow reaches Didymos")
        h["ECLIPS_S"] = (g["eclipse_secondary"],
                         "Dimorphos inside Didymos umbra")
        h["ECLIPS_T"] = (g["eclipse_total_sec"], "that eclipse is total")
        h["OCCULT"] = (g["occultation"], "Dimorphos in front of Didymos disk")
        h["DIMFRONT"] = (g["dimorphos_in_front"], "Dimorphos nearer than Didymos")

        # --- what is in frame ------------------------------------------------
        for i, b in enumerate(BODIES):
            c = coverage[b]
            tag = b[:3].upper()
            h[f"PX_{tag}"] = (c["px"], f"pixels covered by {b}")
            h[f"FOV_{tag}"] = (round(100 * c["fov_frac"], 3),
                               f"[%] of {b} facing area inside the FOV")
            h[f"RES_{tag}"] = (round(100 * c["res_frac"], 3),
                               f"[%] of {b} facing area sampled by a pixel")
            h[f"SRF_{tag}"] = (round(100 * c["total_area_frac"], 3),
                               f"[%] of {b} total surface sampled")
            h[f"FUL_{tag}"] = (c["complete"], f"{b} entirely within the FOV")

        # --- models and method -----------------------------------------------
        for i, b in enumerate(BODIES):
            # Truncated to keep the card inside FITS's 80 columns with its
            # comment intact; the full path goes in a COMMENT below.
            h[f"MESH_{b[:3].upper()}"] = (Path(MESH[b]).name[:38], "shape model")
            h[f"NFAC_{b[:3].upper()}"] = (n_facets[i], f"{b} facet count")
        h["TPMGRID"] = ("geometric-nonuniform", "depth grid")
        h["TPMSTEN"] = ("variable-spacing", "conduction stencil")
        h["TPMSOLV"] = ("explicit-forward-euler", "time integration")
        h["TPMSPIN"] = (3, "solar orbits of spin-up, direct insolation only")
        h["TPMSEG"] = ("6 spins", "high-fidelity segment before the epoch")
        h["TPMDT"] = (55.939, "[s] timestep of the segment")
        h["SHADOW"] = ("mutual", "self-shadowing + mutual eclipse, shadow map")
        h["HEATING"] = ("none", "mutual and self heating NOT included")
        h["REFLECT"] = ("none", "reflected solar omitted, <0.2% in band")
        h["THERMINE"] = (float(kalast.tpm.properties.DIDYMOS.thermal_inertia),
                         "[J/m2/K/s^0.5] thermal inertia")
        h["ALBEDO"] = (float(kalast.tpm.properties.DIDYMOS.albedo), "bond albedo")
        h["RADMETH"] = ("facet-id buffer", "per-pixel facet index, no colormap")
        h["EMISCOS"] = (False, "no emission-angle cosine: Lambertian radiance")
        for b in BODIES:
            h.add_comment(f"{b} mesh: {Path(MESH[b]).name}")
        h.add_comment("Simulated TIRI image produced by kalast.")
        h.add_comment("Pixel value is band-integrated radiance,")
        h.add_comment("integral of B(T,w) * emissivity * R(w) dw over the")
        h.add_comment("filter response, matching calibrated TIRI units.")
        h.add_comment("Zero marks pixels with no body in them.")
        h.add_comment("Mutual and self heating are not modelled; see")
        h.add_comment("notes/2026-08-27_conduction_solvers section 7.")

        path = OUT / f"tiri_didymos_{f}.fits"
        hdu.writeto(path, overwrite=True)
        lit = img[filled]
        print(f"  filter {f} ({band.effective_wavelength * 1e6:5.2f} um, "
              f"{band.bandwidth:5.3f} um wide): "
              f"{lit.min():8.4f} - {lit.max():8.4f} W/m2/sr")

    numpy.save(OUT / "facet_ids.npy", ids)
    numpy.save(OUT / "temperature_map.npy", tmap)
    print(f"\nwrote {OUT}/")


app.before_render = before_render
app.after_render = after_render
app.start()
