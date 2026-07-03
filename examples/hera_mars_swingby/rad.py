#!/usr/bin/env python

# Scaffold: port of examples/old/hera_mars_swingby_old/rad.py to the current
# kalast API. Computes per-facet spectral/band radiance seen by TIRI from
# Deimos surface temperatures already simulated by tpm.py.
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
#   scipy.integrate.simpson(spec*resp, wl) -> kalast.tpm.emit.radiance(f, r, w)
#     (Simpson-integrates a response-weighted spectral radiance; same method
#     as the old script, just wrapped on the Rust side)
#
# NOT YET REPLACED / MISSING -- needs sourcing before this runs end-to-end:
#   - TIRI spectral response function arrays (`resp`, `wlu` in the old code).
#     kalast.entity.TIRI only carries filter *names* (see TIRI.filters in
#     src/entity.rs), not the response curves themselves. The old pipeline
#     loaded these from "work/scene/resp.npy" / "wlu.npy", produced by
#     cam_scene.py / scene.py (not ported). You need to find/regenerate the
#     TIRI filter response calibration data (e.g. from the instrument team)
#     and load it here -- see RESP/WLU placeholders below.
#   - `fwpos` (which filter was used per image) can be rebuilt without the
#     old scene arrays: the images CSV already has a "filter" column
#     (see examples/hera_mars_swingby/main.py loading
#     ".../tiri_images_mars_swing-by_deimos.csv"), matched against
#     kalast.entity.TIRI.filters to get an index.
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
out_dir = Path("out/deimos_tpm")
ets_sim = pandas.read_csv(out_dir / "ets_sim.csv")["time"].to_numpy()
tmp_surf = pandas.read_csv(out_dir / "tmp_surf.csv").to_numpy()
assert tmp_surf.shape[1] == nface, "tmp_surf.csv facet count != mesh facet count"

# --- Images to simulate: reuse the same CSV as main.py ---
df_images = pandas.read_csv(
    "/Users/gregoireh/data/hera/tiri/tiri_images_mars_swing-by_deimos.csv"
)
images = df_images["image"].to_list()
et_images = df_images["et"].to_numpy()
filter_images = df_images["filter"].to_list()
fwpos = numpy.array([cam.filters.index(f) for f in filter_images])

# --- TODO: load real TIRI spectral response function per filter ---
# RESP: shape (n_wavelengths, n_filters), WLU: shape (n_wavelengths,) in microns
# wl = WLU * 1e-6
# resp = RESP
raise NotImplementedError(
    "Load TIRI response function (resp, wlu) before running -- see header comment."
)

n = len(images)
nw = wl.size

rad_all = numpy.zeros((n, nface))
irrad_all = numpy.zeros((n, nface))
tmp_all = numpy.zeros((n, nface))
inc_all = numpy.zeros((n, nface))
emi_all = numpy.zeros((n, nface))

# Roughness correction placeholder (old code also left this as ones).
R = numpy.ones((n, nface))

for it, image in enumerate(images):
    et = et_images[it]
    ii_simu = numpy.argmin(numpy.abs(ets_sim - et))

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
        cosi = kalast.math.cosine_incidence(u_sun, n_)

        v_sc = p_sc - p
        d_sc = numpy.linalg.norm(v_sc)
        u_sc = v_sc / d_sc
        cose = kalast.math.cosine_incidence(u_sc, n_)

        tmp_ = tmp_surf[ii_simu, iif]

        spec_rad = numpy.array([kalast.tpm.emit.planck(tmp_, w_) for w_ in wl])
        spec_rad = kalast.tpm.emit.spectral_radiance(
            spec_rad, prop_emissivity := 0.95, cose, R[it, iif]
        )

        rad_ = kalast.tpm.emit.radiance(spec_rad, resp[:, fwpos[it]], wl)
        sr_ = kalast.tpm.emit.steradian(area, d_sc)
        irrad_ = kalast.tpm.emit.irradiance(rad_, sr_)

        rad_all[it, iif] = rad_
        irrad_all[it, iif] = irrad_
        tmp_all[it, iif] = tmp_
        inc_all[it, iif] = numpy.arccos(cosi)
        emi_all[it, iif] = numpy.arccos(cose)

numpy.save(out_dir / "rad_all.npy", rad_all)
numpy.save(out_dir / "irrad_all.npy", irrad_all)
numpy.save(out_dir / "tmp_all.npy", tmp_all)
numpy.save(out_dir / "inc_all.npy", inc_all)
numpy.save(out_dir / "emi_all.npy", emi_all)
