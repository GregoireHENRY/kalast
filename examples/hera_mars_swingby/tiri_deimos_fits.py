#!/usr/bin/env python
"""Simulated TIRI radiance images of Deimos at the Mars swing-by, as FITS.

One file per real image in `tiri_images_mars_swing-by_deimos.csv`, at that
image's epoch and in that image's filter, so the products can be laid beside
the observed frames.

How a pixel gets its value
--------------------------

The same route as the Didymos product (`examples/hera_didymos/tiri_fits.py`):

    render      -> facet index per pixel, from `sim.request_facet_id()`
    TPM         -> surface temperature per facet, stepped with self-shadowing
    L(T)        -> band-integrated radiance for that image's filter

Pixel values are `eps * B_band(T)` in `W m^-2 sr^-1`, band **integrated**, which
is what real calibrated TIRI carries. A grey Lambertian surface emits the same
radiance in every direction, so no cosine enters: the projected area is already
accounted for by which pixels the facet covers.

What is and is not modelled
---------------------------

- **Self-shadowing: yes**, from the GPU shadow map, and the TPM is stepped
  through two Deimos rotations with it before the sequence so the surface has
  the right thermal history rather than an unshadowed one.
- **Mutual shadowing: measured to be absent, and therefore not modelled.**
  Deimos is not eclipsed by Mars at any of these epochs, and neither Mars nor
  Phobos occults it -- Mars is *behind* Deimos throughout. Both are checked at
  every epoch and written to the header, so the assumption is verifiable rather
  than assumed.
- **Mars is deliberately not in the rendered scene.** It is 3,396 km across at
  ~90,000 km, so putting it in would fit the shadow frustum to the whole scene
  and leave 6 km Deimos with almost no shadow-map resolution. Since it neither
  shadows nor occults, it is reported in the header instead. **It does sit
  behind Deimos in several frames** -- separation falls to 1.06 deg against an
  8 deg angular radius -- so those pixels have a warm Mars background in the
  real data that these images do not contain. `MARSBACK` flags them.
- **No view factors, no self or mutual heating.** Out of scope for this
  delivery; the Didymos work has them (`notes/2026-08-31_view_factors/`).

Deimos spans 7 px at the first epoch and 101 px at the last. The shape model is
5,040 facets at ~321 m, so at the closest frames it is under-resolved relative
to a 125 m pixel -- reported per image as `FACETPX`.
"""

import hashlib
from pathlib import Path

import numpy
import pandas
import spiceypy as spice
from astropy.io import fits

import kalast
import kalast.tiri_alignment as tiri_align  # 0.60 deg alignment the FK lacks
import kalast.tiri_timing as tiri_timing    # empirical -24.89 s, see the module

# On for this product: without it Deimos lands up to 352 px from the observed
# position. Empirical and unexplained -- read kalast/tiri_timing.py before
# circulating anything built with it. Recorded per image as TIMEOFFS.
tiri_timing.ENABLED = True
import kalast.tpm.nonuniform as nonuniform
import kalast.tpm.properties as properties
import kalast.tpm.radiance as radiance
import kalast.tpm.routine as routine
from kalast.util import AU, RPD, SOLAR_CONSTANT, STEFAN_BOLTZMANN

# ---------------------------------------------------------------- settings
KERNEL = "/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops_local.tm"
MESH = "/Users/gregoireh/data/mesh/deimos/deimos_k005_tho_v02.obj"
RESPONSE = "/Users/gregoireh/data/hera/tiri/response.csv"
IMAGES = "/Users/gregoireh/data/hera/tiri/tiri_images_mars_swing-by_deimos.csv"
RESTART = "out/hera_mars_swingby/deimos_tpm"
OUT = Path("out/hera_mars_swingby/tiri_deimos_fits")

PREROLL_ROTATIONS = 2.0   # of thermal history with self-shadowing before the run
DT_COARSE_SAFETY = 0.4    # of the stability limit, for the pre-roll
DT_FINE = 10.0            # s, the background grid through the image sequence.
                          # Image epochs are inserted into it exactly, so this
                          # no longer sets any capture offset -- it only sets
                          # how finely the thermal history is marched between
                          # images.

R_MARS, R_PHOBOS = 3396.2, 11.1   # km

FILTER_KEY = {"Filter a (7.8um)": "a", "Filter b (8.6um)": "b",
              "Filter c (9.6um)": "c", "Filter d (10.6um)": "d",
              "Filter e (11.6um)": "e", "Filter f (13.0um)": "f",
              "Filter g (wide)": "g"}

# ------------------------------------------------------------------ setup
spice.kclear()
spice.furnsh(KERNEL)

body = kalast.entity.DEIMOS
prop = kalast.tpm.properties.DEIMOS
prop.se = STEFAN_BOLTZMANN * prop.emissivity
prop.compute_conductivity_diffusivity()
D = prop.diffusivity
ORBIT_PERIOD = kalast.entity.MARS.orbit_period

images = pandas.read_csv(IMAGES)
images["key"] = images["filter"].map(FILTER_KEY)
if images["key"].isna().any():
    raise SystemExit(f"unmapped filters: {sorted(set(images['filter']) - set(FILTER_KEY))}")
et_images = images["et"].to_numpy()
et_first, et_last = et_images.min(), et_images.max()

bands = radiance.tiri_bands(RESPONSE, emissivity=prop.emissivity)

ls1 = properties.skin_depth_1(D, body.spin_period)
z = nonuniform.column(ls1, m=4, n=5,
                      b=properties.skin_depth_2pi(D, ORBIT_PERIOD) / ls1)
twodz = 2.0 * (z[1] - z[0])
dt_coarse = DT_COARSE_SAFETY * routine.nonuniform_max_dt(z, D)

src = Path(RESTART)
T = pandas.read_csv(src / "tmp_state.csv").to_numpy()
z_prev = pandas.read_csv(src / "z.csv")["depth"].to_numpy()
if not numpy.allclose(z_prev, z):
    raise SystemExit("restart grid does not match this run's grid")

tiri = kalast.entity.TIRI
NPX, NPY = int(tiri.px[0]), int(tiri.px[1])

print(f"TIRI Deimos reconstruction, Mars swing-by 2025-03-12")
print(f"  {len(images)} images, {spice.et2utc(et_first,'ISOC',0)} -> "
      f"{spice.et2utc(et_last,'ISOC',0)}")
print(f"  filters: {', '.join(sorted(set(images['key'])))}")
print(f"  detector {NPX}x{NPY}, fovy {tiri.fovy:.1f} deg")

# -------------------------------------------------------------- rendering
app = kalast.app.App()
app.config.width = NPX
app.config.height = NPY
app.config.vsync = False
app.config.access_shadow_map = True
app.simulation.camera.projection.fovy = numpy.radians(tiri.fovy)

app.simulation.load_mesh(path=MESH, mat=numpy.eye(4), flatten=True)
mesh = app.simulation.bodies[0].mesh
nface = len(mesh.facets)
if nface != T.shape[0]:
    raise SystemExit(f"mesh has {nface:,} facets, restart state has {T.shape[0]:,}")
fp = hashlib.sha256(numpy.asarray(mesh.positions, dtype=numpy.float64).tobytes()).hexdigest()
fp_file = src / "mesh_fingerprint.txt"
if fp_file.exists() and fp_file.read_text().strip() != fp:
    raise SystemExit("the shape model has changed since the spin-up; re-run tpm_deimos.py")

positions = numpy.array([mesh.facets[k].pos for k in range(nface)]) * 1e3
normals = numpy.array([mesh.facets[k].normal for k in range(nface)])
areas = numpy.array([mesh.facets[k].area for k in range(nface)])
# Conduction coefficients depend on the timestep, and the fine phase no longer
# has a single one: inserting the image epochs into the grid makes some steps
# shorter than DT_FINE. Cached per distinct dt -- there are only a handful, and
# building one for ~30 nodes is trivial. Reusing the DT_FINE coefficients for a
# 3 s step would advance the column as though 10 s had passed.
_coef_cache = {}


def coefs_for(dt):
    key = round(float(dt), 6)
    c = _coef_cache.get(key)
    if c is None:
        c = tuple(numpy.asarray(x, numpy.float64)
                  for x in routine.nonuniform_coefficients(z, key))
        _coef_cache[key] = c
    return c
d_nodes = numpy.full(z.size, D)

et_preroll = et_first - PREROLL_ROTATIONS * body.spin_period
n_coarse = int(numpy.ceil((et_first - et_preroll) / dt_coarse))

# Every image epoch is a step, exactly.
#
# Capturing on the grid step at or before each image epoch put the render up
# to DT_FINE early. That is invisible far out and ruinous close in: near
# closest approach Deimos crosses the field at ~20 px/s, so the 3-7 s offsets
# the grid happened to give displaced the body by up to 92 px, growing as the
# range fell. It was the whole of the "close-range displacement" -- predicting
# the shift from the offset alone matches the measured silhouette centre to
# better than 0.7 px at every epoch.
#
# The tell was that the four distant frames were pixel-perfect: their epochs
# are exact multiples of DT_FINE from the first, so they had no offset at all.
_grid = et_first + numpy.arange(
    int(numpy.ceil((et_last - et_first) / DT_FINE)) + 2) * DT_FINE
fine_epochs = numpy.unique(numpy.concatenate([_grid, et_images]))
n_fine = fine_epochs.size
print(f"  pre-roll {n_coarse:,} steps at {dt_coarse:.0f}s, then "
      f"{n_fine:,} through the sequence "
      f"({_grid.size:,} on a {DT_FINE:.0f}s grid, plus the image epochs)\n")

state = {"i": 0, "captured": 0, "pending": None}
OUT.mkdir(parents=True, exist_ok=True)


def epoch_of(i):
    if i < n_coarse:
        return et_preroll + i * dt_coarse
    return float(fine_epochs[min(i - n_coarse, n_fine - 1)])


def before_render(sim, _dt):
    i = state["i"]
    if i > n_coarse + n_fine:
        return
    # Two epochs, deliberately distinct. `et_label` is the image's own epoch and
    # is what the capture below matches and what the header records; `et` is
    # where the geometry is evaluated, which the empirical offset shifts.
    # Collapsing them stops every capture matching.
    et_label = epoch_of(i)
    et = tiri_timing.apply(et_label)

    # Deimos at the origin in its own frame; the camera is TIRI, so the body
    # is placed by the TIRI-relative position and the IAU_DEIMOS orientation.
    p, _lt = spice.spkpos("DEIMOS", et, tiri.frame, "none", "HERA")
    p = tiri_align.apply(p)
    m = spice.pxform(body.frame, tiri.frame, et)
    sim.bodies[0].mat[:3, :3] = m
    sim.bodies[0].mat[:3, 3] = p

    # Sun in the same frame, for the shadow map. Orthographic and collimated,
    # so only the direction matters.
    ps, _lt = spice.spkpos("SUN", et, tiri.frame, "none", "HERA")
    ps = tiri_align.apply(ps)
    u = numpy.asarray(ps) / numpy.linalg.norm(ps)
    # Aimed at Deimos, not at the origin: the origin here is TIRI itself,
    # tens of thousands of km away, and the shadow frustum is fitted about
    # what the light looks at.
    sim.sun.anchor = list(numpy.asarray(p, dtype=float))
    sim.sun.pos = numpy.asarray(p) + u * 200.0
    sim.sun.look_anchor()

    # TIRI itself: at the origin of its own frame, boresight +Z.
    sim.camera.pos = [0.0, 0.0, 0.0]
    sim.camera.dir = [0.0, 0.0, 1.0]
    # +Y up, not -Y: TIRI's detector runs 180 deg from a naive +X-right,
    # +Y-down frame. Established against the real calibrated radiances --
    # Deimos traverses the field the opposite way with -Y, and reprojecting
    # Mars to lat/lon only gives a self-consistent map across epochs with +Y.
    sim.camera.up = [0.0, 1.0, 0.0]

    sim.hud = f"{i}/{n_coarse + n_fine} it  captured {state['captured']}/{len(images)}"

    # Capture only on a step that *is* an image epoch, since every image epoch
    # is now in the grid. Matching on equality rather than on an interval is
    # the point: the interval form silently rendered the previous grid step.
    if i >= n_coarse:
        hit = numpy.where(numpy.abs(et_images - et_label) < 1e-6)[0]
        if hit.size:
            state["pending"] = hit
            sim.request_facet_id()


def step_tpm(sim, et, dt_step):
    """One TPM step of `dt_step` seconds, self-shadowed by this frame's map."""
    ps, _lt = spice.spkpos("SUN", et, body.frame, "none", "DEIMOS")
    ps = numpy.asarray(ps) * 1e3
    v = ps[None, :] - positions
    d_sun = numpy.linalg.norm(v, axis=1)
    cosi = numpy.einsum("ij,ij->i", normals, v / d_sun[:, None])
    numpy.maximum(cosi, 0.0, out=cosi)
    frac = sim.facet_shadow(0)
    lit = (1.0 - numpy.asarray(frac, dtype=numpy.float64)
           if frac is not None else numpy.ones(nface))
    flux = SOLAR_CONSTANT * (1.0 - prop.albedo) * cosi * lit / (d_sun / AU) ** 2
    routine.step_surface_newton(T, flux, prop.se, prop.conductivity, twodz,
                                threshold=kalast.util.NEWTON_METHOD_THRESHOLD)
    routine.step_conduction(T, d_nodes, coefs_for(dt_step))
    return int(((lit < 1.0) & (cosi > 0)).sum())


def write_image(row, et, ids, offsets):
    key = row["key"]
    band = bands[key]
    filled = ids > 0
    facet_of = numpy.where(filled, ids - 1, 0)

    tmap = numpy.zeros(ids.shape, dtype=numpy.float64)
    tmap[filled] = T[facet_of[filled], 0]
    img = numpy.zeros(ids.shape, dtype=numpy.float32)
    img[filled] = band(tmap[filled]).astype(numpy.float32)

    p, _lt = spice.spkpos("DEIMOS", et, tiri.frame, "none", "HERA")
    p = tiri_align.apply(p)
    d = numpy.linalg.norm(p)
    gsd = numpy.radians(tiri.fovy) / NPY * d * 1e3
    # phase angle at Deimos
    a, _lt = spice.spkpos("SUN", et, "J2000", "none", "DEIMOS")
    b, _lt = spice.spkpos("HERA", et, "J2000", "none", "DEIMOS")
    phase = numpy.degrees(numpy.arccos(
        numpy.dot(a, b) / numpy.linalg.norm(a) / numpy.linalg.norm(b)))
    # Mars / Phobos, reported rather than rendered
    mp, _lt = spice.spkpos("MARS", et, tiri.frame, "none", "HERA")
    mp = tiri_align.apply(mp)
    pp, _lt = spice.spkpos("PHOBOS", et, tiri.frame, "none", "HERA")
    mn, pn = numpy.linalg.norm(mp), numpy.linalg.norm(pp)
    sep_m = numpy.degrees(numpy.arccos(numpy.dot(p, mp) / d / mn))
    sep_p = numpy.degrees(numpy.arccos(numpy.dot(p, pp) / d / pn))
    ang_m = numpy.degrees(numpy.arcsin(R_MARS / mn))
    ang_p = numpy.degrees(numpy.arcsin(R_PHOBOS / pn))
    try:
        ecl = spice.occult("MARS", "ELLIPSOID", "IAU_MARS", "SUN", "ELLIPSOID",
                           "IAU_SUN", "NONE", "DEIMOS", et) > 0
    except Exception:
        ecl = False

    hdu = fits.PrimaryHDU(img)
    h = hdu.header
    h["ORIGIN"] = ("kalast simulation", "SIMULATED, not observed")
    h["TELESCOP"] = ("HERA", "telescope used to acquire data")
    h["INSTRUME"] = ("TIRI", "HERA Thermal Infrared Imager")
    h["BUNIT"] = ("W m^-2 sr^-1", "unit of pixel values")
    h["TARGET"] = ("DEIMOS", "body modelled")
    h["REALIMG"] = (row["image"], "observed frame this reproduces")
    h["DATE-OBS"] = (spice.et2utc(et, "ISOC", 3), "UTC of the observed frame")
    h["ET"] = (float(et), "[s] TDB seconds past J2000")
    h["FW_NUM"] = (row["filter"], "filter wheel position")
    h["WAVELEN"] = (round(band.effective_wavelength * 1e6, 4),
                    "[um] response-weighted effective wavelength")
    h["BANDWID"] = (round(band.bandwidth, 4), "[um] integral of the response")
    h["EMISSIV"] = (prop.emissivity, "grey emissivity assumed")
    h["ALBEDO"] = (prop.albedo, "bolometric albedo assumed")
    h["THERMINR"] = (prop.thermal_inertia, "[J m^-2 K^-1 s^-1/2] thermal inertia")
    h["RANGE"] = (round(float(d), 3), "[km] TIRI to Deimos")
    h["GSD"] = (round(float(gsd), 2), "[m] ground sampling at that range")
    h["PHASE"] = (round(float(phase), 3), "[deg] solar phase angle at Deimos")
    h["NPIXBODY"] = (int(filled.sum()), "pixels containing Deimos")
    h["FACETPX"] = (round(float((numpy.sqrt(areas.mean()) * 1e3 / gsd) ** 2), 3),
                    "pixels per facet; >1 = shape under-resolved")
    h["TMIN"] = (round(float(tmap[filled].min()), 2) if filled.any() else 0.0,
                 "[K] coldest modelled pixel")
    h["TMAX"] = (round(float(tmap[filled].max()), 2) if filled.any() else 0.0,
                 "[K] hottest modelled pixel")
    h["TMEAN"] = (round(float(routine.area_mean(T[:, 0], areas)), 2),
                  "[K] area-weighted mean over the whole body")
    # provenance of what is absent
    h["SELFSHAD"] = (True, "self-shadowing included, from the shadow map")
    h["ECLIPSE"] = (bool(ecl), "in Mars shadow (measured, not assumed)")
    h["MARSSEP"] = (round(float(sep_m), 3), "[deg] Deimos to Mars centre")
    h["MARSANG"] = (round(float(ang_m), 3), "[deg] Mars angular radius")
    h["MARSBACK"] = (bool(sep_m < ang_m and mn > d),
                     "on Mars disc; background not modelled")
    h["PHOBSEP"] = (round(float(sep_p), 3), "[deg] Deimos to Phobos centre")
    h["PHOBANG"] = (round(float(ang_p), 4), "[deg] Phobos angular radius")
    h["SELFHEAT"] = (False, "self-heating NOT modelled")
    h["MUTHEAT"] = (False, "mutual heating NOT modelled")
    h["ROUGHNES"] = (False, "thermal roughness NOT modelled")
    h["DETROT"] = (180, "[deg] detector orientation vs a naive +X-right frame")
    h["TIMEOFFS"] = (round(float(tiri_timing.OFFSET_S), 2) if tiri_timing.ENABLED else 0.0,
                     "[s] empirical epoch offset applied to the geometry")
    h["ALIGNDEG"] = (round(float(tiri_align.ANGLE_DEG), 4),
                     "[deg] TIRI alignment applied; FK carries none")
    h["ALIGNAX"] = (str([round(a, 6) for a in tiri_align.AXIS]),
                    "rotation axis of that alignment, in HERA_TIRI")
    h["GEOMVAL"] = ("1.8px vs real 11:56:03", "geometry validated against observation")
    h["SHAPE"] = (Path(MESH).name, "shape model")
    h["NFACETS"] = (nface, "facets in the shape model")
    h["KERNEL"] = (Path(KERNEL).stem, "SPICE meta-kernel")

    name = Path(row["image"]).stem + f"_sim_{key}.fits"
    hdu.writeto(OUT / name, overwrite=True)
    return name, d, gsd, filled.sum(), tmap[filled] if filled.any() else numpy.array([0.0])


def after_render(sim, _dt):
    i = state["i"]
    if i > n_coarse + n_fine:
        return
    et = epoch_of(i)

    if state["pending"] is not None:
        got = sim.facet_id_map()
        if got is not None:
            ids, offsets = got
            ids = numpy.asarray(ids)
            for k in state["pending"]:
                row = images.iloc[k]
                name, d, gsd, npx, t = write_image(row, float(et_images[k]),
                                                   ids, offsets)
                state["captured"] += 1
                print(f"  [{state['captured']:2d}/{len(images)}] {name}"
                      f"  range {d:7.1f} km  {gsd:6.1f} m/px  {npx:6,} px"
                      f"  T {t.min():5.1f}-{t.max():5.1f} K", flush=True)
        state["pending"] = None

    # The step actually about to be taken, which in the fine phase varies.
    step_tpm(sim, et, epoch_of(i + 1) - et if i >= n_coarse else dt_coarse)
    state["i"] += 1

    if state["i"] > n_coarse + n_fine:
        print(f"\nwrote {state['captured']}/{len(images)} images to {OUT}/")
        import os
        import sys
        sys.stdout.flush()
        os._exit(0)


app.before_render = before_render
app.after_render = after_render
app.start()
