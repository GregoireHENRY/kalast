#!/usr/bin/env python

# Port of examples/old/hera_mars_swingby_old/rad.py to the current kalast
# API. Computes per-facet spectral/band radiance seen by TIRI from Deimos
# surface temperatures already simulated by tpm.py.
#
# Wide band filter only for now (TIRI "Filter g (wide)" / Response_Fil-g in
# response.csv). Per-image/per-filter selection (fwpos) is not done yet --
# see TODO below.
#
# Direct replacements applied (old -> new):
#   trimesh.load(...) + mesh.triangles_center/face_normals/area_faces
#       -> kalast.mesh.Mesh(path), mesh.facets[i].pos/.normal/.area
#          (must be built with the SAME mesh file + .flatten() as tpm.py,
#          so facet ordering/count matches tmp_surf.csv columns)
#   kalast.spice_entities.hera/tiri/deimos -> kalast.entity.HERA/TIRI/DEIMOS
#   kalast.astro.cosinc(u, n)              -> kalast.math.cosine_incidence(u, n)
#   kalast.tpm.core.planck(t, wl)          -> kalast.tpm.emit.planck(t, w)
#   manual ster_ = area / d**2             -> kalast.tpm.emit.steradian(area, d)
#   manual rad_ * ster_                    -> kalast.tpm.emit.irradiance(f, sr)
#   RESP/WLU npy arrays                    -> /Users/gregoireh/data/hera/tiri/response.csv
#     ("#Wavelength[um]" + one "Response_Fil-x" column per filter)
#
# Note: kalast.tpm.emit.planck/spectral_radiance/radiance all take a scalar
# wavelength/spectral-radiance (see src/tpm/emit.rs), not an array -- they
# are NOT a drop-in replacement for scipy.integrate.simpson(spec*resp, wl).
# The per-wavelength spectral radiance is still built with a Python loop
# calling planck/spectral_radiance scalar-wise (as in the older, already
# ported examples/old/hera_necp_moon/rad.py), then band-integrated with
# scipy.integrate.simpson directly.
#
# STILL TODO:
#   - Only the wide filter (Response_Fil-g) is used, applied to every TPM
#     timestep. To simulate actual TIRI images instead, switch the loop to
#     iterate over the images CSV (see commented block below) and pick the
#     response column matching each image's filter, matched against
#     kalast.entity.TIRI.filters.
#   - Camera/observer position per image (`po`, `ds` in the old code): can be
#     rebuilt with spice.spkpos(HERA.name, et, DEIMOS.frame, "none", DEIMOS.name),
#     as already done for the sun vector in tpm.py -- scaffolded below.
#   - Roughness correction `R`: old code just used ones (not actually
#     computed there either) -- kept as a placeholder here too.
#   - Pixel-grid projection (rad_campx.py logic, turning per-facet radiance
#     into an actual image raster via kalast.spice.cam_cpt_surf_2) is not
#     scaffolded here -- that function is still available as-is in
#     kalast/spice.py (cam_cpt_surf / cam_cpt_surf_2 / fovcov), untouched
#     since the old pipeline, and can be reused directly for that step.

from pathlib import Path  # noqa

import numpy
import pandas
import scipy
import spiceypy as spice

import kalast
from kalast.util import AU, DPR  # noqa

# --- Spice setup (mirrors tpm.py) ---
spice.kclear()
spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops_local.tm")

sc = kalast.entity.HERA
cam = kalast.entity.TIRI
deimos = kalast.entity.DEIMOS

# --- Mesh: must match tpm.py's mesh file + flatten() exactly ---
mesh_file = "/Users/gregoireh/data/spice/hera/kernels/dsk/deimos_k005_tho_v02.obj"
mesh = kalast.mesh.Mesh(mesh_file)
mesh.flatten()
nface = len(mesh.facets)

# --- TPM output (from tpm.py, e.g. a saved run copied into out/deimos_tpm) ---
tpm_dir = Path("out/hera_mars_swingby/deimos_tpm_3")
ets_sim = pandas.read_csv(tpm_dir / "ets_sim.csv")["time"].to_numpy()
tmp_surf = pandas.read_csv(tpm_dir / "tmp_surf.csv").to_numpy()
assert tmp_surf.shape[1] == nface, "tmp_surf.csv facet count != mesh facet count"

# --- Images to simulate: reuse the same CSV as main.py ---
# TODO later, for the moment use TPM ets_sim directly (one "image" per
# TPM-recorded timestep, all with the wide filter).
# df_images = pandas.read_csv(
#     "/Users/gregoireh/data/hera/tiri/tiri_images_mars_swing-by_deimos.csv"
# )
# images = df_images["image"].to_list()
# et_images = df_images["et"].to_numpy()
# filter_images = df_images["filter"].to_list()
# fwpos = numpy.array([cam.filters.index(f) for f in filter_images])

# --- TIRI spectral response function (wide filter only for now) ---
df_resp = pandas.read_csv("/Users/gregoireh/data/hera/tiri/response.csv")
wl = df_resp["#Wavelength[um]"].to_numpy() * 1e-6
resp = df_resp["Response_Fil-g"].to_numpy()
nw = wl.size

nit = len(ets_sim)

rad_all = numpy.zeros((nit, nface))
irrad_all = numpy.zeros((nit, nface))
tmp_all = numpy.zeros((nit, nface))
inc_all = numpy.zeros((nit, nface))
emi_all = numpy.zeros((nit, nface))

# Roughness correction placeholder (old code also left this as ones).
R = numpy.ones((nit, nface))

# Time loop progress (same pattern as tpm.py).
progress_freq = "1"
digits = [len(_d) for _d in progress_freq.split(".")]
digits_full = 3
digits_decimal = 0
if len(digits) == 2:
    digits_decimal = digits[1]
    if digits_decimal > 0:
        digits_full += digits_decimal + 1
freqv = float(progress_freq)
last_freq_reached = -freqv
ndigits = kalast.util.numdigits_comma(freqv)
digit = 10**ndigits

for it, et in enumerate(ets_sim):
    # interpolation between ets_sim from TPM simu and real TIRI images
    # ii_simu = numpy.argmin(numpy.abs(ets_sim - et))

    (p_sc, _lt) = spice.spkpos(sc.name, et, deimos.frame, "none", deimos.name)
    p_sc *= 1e3
    (p_sun, _lt) = spice.spkpos("sun", et, deimos.frame, "none", deimos.name)
    p_sun *= 1e3

    for iif in range(nface):
        p = mesh.facets[iif].pos
        n_ = mesh.facets[iif].normal
        area = mesh.facets[iif].area

        v_sun = p_sun - p
        d_sun = numpy.linalg.norm(v_sun)
        u_sun = v_sun / d_sun
        cosi = kalast.math.cosine_incidence(u_sun.astype(numpy.float32), n_)

        v_sc = p_sc - p
        d_sc = numpy.linalg.norm(v_sc)
        u_sc = v_sc / d_sc
        cose = kalast.math.cosine_incidence(u_sc.astype(numpy.float32), n_)

        tmp_ = tmp_surf[it, iif]
        emissivity = 0.95

        # kalast.tpm.emit.planck/spectral_radiance take a scalar wavelength,
        # so build the per-wavelength spectral radiance array in Python (same
        # pattern as examples/old/hera_necp_moon/rad.py), then band-integrate
        # against the response function with scipy (kalast.tpm.emit.radiance
        # also expects a scalar f, not an array -- not usable here).
        spec_rad = numpy.array(
            [
                kalast.tpm.emit.spectral_radiance(
                    kalast.tpm.emit.planck(tmp_, w_), emissivity, cose, R[it, iif]
                )
                for w_ in wl
            ]
        )

        rad_ = scipy.integrate.simpson(spec_rad * resp, wl)
        sr_ = kalast.tpm.emit.steradian(area, d_sc)
        irrad_ = kalast.tpm.emit.irradiance(rad_, sr_)

        rad_all[it, iif] = rad_
        irrad_all[it, iif] = irrad_
        tmp_all[it, iif] = tmp_
        inc_all[it, iif] = numpy.arccos(cosi)
        emi_all[it, iif] = numpy.arccos(cose)

    # Show progress
    progress = it / (nit - 1) * 100
    if ndigits > 0:
        progress = numpy.floor(progress * digit) / digit
    if progress >= last_freq_reached + freqv:
        last_freq_reached += freqv
        print(f"{progress:{digits_full}.{digits_decimal}f}% ({it:,}/{nit - 1:,}it)")


def save_per_facet_csv(name, data):
    # Same row/col convention as tpm.py's tmp_surf.csv: one row per
    # timestep, one column per facet index (0..nface-1).
    df = {}
    for iif in range(nface):
        df[iif] = data[:, iif]
    df = pandas.DataFrame(df)
    df.to_csv(tpm_dir / f"{name}.csv", index=False, encoding="utf-8-sig")


save_per_facet_csv("rad_all", rad_all)
save_per_facet_csv("irrad_all", irrad_all)
save_per_facet_csv("tmp_all", tmp_all)
save_per_facet_csv("inc_all", inc_all)
save_per_facet_csv("emi_all", emi_all)
