"""Disc-integrated Deimos radiance across the swing-by, and a roughness sweep.

Three things this does differently from summing a rendered frame.

**The camera is not used at all.** At 7,600 km Deimos spans ~6.9 px, so a
1024x768 render puts ~35 pixels on a body with ~2,600 visible facets and each
pixel carries one facet's radiance. That is a 35-sample Monte Carlo estimate of
the disc integral: two renders of the same epoch at different sub-pixel phase
disagreed by 11%. The integral is just a sum over facets,

    F = sum  L(T_f) * R_f * cos(e_f) * A_f / d^2

so it is done directly, exactly, and instantly. The renderer is still needed,
but only for the shadow map.

**Self-shadowing has to be added here.** `tpm_deimos.py` builds insolation as
`S (1-A) cos(i) / r^2` with no `lit` factor, so its converged restart state has
no self-shadowing and facets in shadowed concavities sit too warm. Prerolling
with `facet_shadow` is what fixes that -- it moves the 10th percentile of the
visible surface from 150 K to ~104 K -- and it is why the preroll cannot simply
be skipped.

**The preroll runs at the stability limit.** `nonuniform_max_dt` allows 1876 s
here and the spinup itself uses 0.4x that; the frame script's 10 s was 187x
smaller than needed, turning two rotations into 21,824 steps instead of 290.

The prerolled state is saved, so the roughness sweep and any re-analysis cost
no renders at all.

Run:  python examples/hera_mars_swingby/tiri_deimos_photometry.py
"""

import sys
from pathlib import Path

import numpy
import pandas
import spiceypy as spice

import kalast
import kalast.tiri_timing as tiri_timing
import kalast.tpm.nonuniform as nonuniform
import kalast.tpm.properties as properties
import kalast.tpm.radiance as radiance
import kalast.tpm.routine as routine
from kalast._rs.tpm.roughness import Crater, rms_slope_deg
from kalast.util import AU, SOLAR_CONSTANT, STEFAN_BOLTZMANN

KERNEL = "/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops_local.tm"
MESH = "/Users/gregoireh/data/mesh/deimos/deimos_k005_tho_v02.obj"
RESTART = "out/hera_mars_swingby/deimos_tpm"
OUT = Path("out/hera_mars_swingby/photometry")
STATE = OUT / "prerolled_state.csv"

# Epochs where Mars is out of shot, so aperture photometry on the real frame is
# clean. Fluxes measured from the calibrated radiances, background subtracted,
# aperture r=25 px (the curve of growth is flat from r~10).
EPOCHS = [("115603", "2025-03-12T11:56:03", 3.2944e-05),
          ("115843", "2025-03-12T11:58:43", 4.8846e-05),
          ("120123", "2025-03-12T12:01:23", 8.0794e-05),
          ("120403", "2025-03-12T12:04:03", 1.5520e-04)]

import os as _os
PREROLL_ROT = float(_os.environ.get("PREROLL_ROT", "2.0"))
# Sized against the conduction stability limit, but the surface Newton step is
# the stiffer constraint in practice -- see the dt convergence check in
# notes/2026-09-02_deimos_photometry/. Overridable so that check is repeatable.
DT_SAFETY = float(_os.environ.get("DT_SAFETY", "0.4"))
QUIET = _os.environ.get("QUIET", "") == "1"
OPENINGS = [60.0, 90.0, 120.0, 150.0, 180.0]
COVERAGES = [0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0]

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
DT = DT_SAFETY * routine.nonuniform_max_dt(z, D)
T = pandas.read_csv(Path(RESTART) / "tmp_state.csv").to_numpy()
tiri_timing.ENABLED = True   # match the FITS product

ET0 = spice.str2et("2025-03-12 12:00:00 UTC")     # the restart's own epoch
OUT.mkdir(parents=True, exist_ok=True)

app = kalast.app.App()
app.config.width = 512
app.config.height = 512
app.config.vsync = False
app.config.access_shadow_map = True
app.simulation.load_mesh(path=MESH, mat=numpy.eye(4), flatten=True)
nface = len(app.simulation.bodies[0].mesh.facets)
pos = numpy.array([app.simulation.bodies[0].mesh.facets[k].pos
                   for k in range(nface)]) * 1e3
nrm = numpy.array([app.simulation.bodies[0].mesh.facets[k].normal
                   for k in range(nface)])
area_m2 = numpy.array([app.simulation.bodies[0].mesh.facets[k].area
                       for k in range(nface)]) * 1e6
coefs = tuple(numpy.asarray(c, numpy.float64)
              for c in routine.nonuniform_coefficients(z, DT))
d_nodes = numpy.full(z.size, D)
n_pre = int(PREROLL_ROT * body.spin_period / DT)
st = {"i": 0}

print(f"dt = {DT:.1f} s (stability limit {routine.nonuniform_max_dt(z, D):.0f} s, "
      f"safety {DT_SAFETY}); {PREROLL_ROT} rotations = {n_pre:,} steps")
print(f"{nface:,} facets, total area {area_m2.sum()/1e6:.2f} km^2")
print("prerolling from the restart state to add self-shadowing, which the "
      "spinup omits\n")


def sweep():
    """Analytic disc-integrated flux per epoch, over the roughness grid."""
    rows = []
    print(f"\n{'epoch':<12}{'range km':>10}{'phase':>7}{'F_smooth':>12}"
          f"{'F_real':>12}{'need':>8}")
    per = {}
    for lab, utc, f_real in EPOCHS:
        # Same empirical offset the FITS product uses, so the geometry the
        # photometry is evaluated at matches the images being compared.
        et = tiri_timing.apply(spice.str2et(utc))
        ob = numpy.asarray(spice.spkpos("HERA", et, body.frame, "none", "DEIMOS")[0])
        sb = numpy.asarray(spice.spkpos("SUN", et, body.frame, "none", "DEIMOS")[0])
        d_m = float(numpy.linalg.norm(ob)) * 1e3
        oh = ob / numpy.linalg.norm(ob)
        sh = sb / numpy.linalg.norm(sb)
        ce = nrm @ oh
        cs = nrm @ sh
        vis = ce > 0.0
        stg = sh[None, :] - cs[:, None] * nrm
        otg = oh[None, :] - ce[:, None] * nrm
        ns = numpy.linalg.norm(stg, axis=1)
        no = numpy.linalg.norm(otg, axis=1)
        ok = (ns > 1e-9) & (no > 1e-9)
        cpsi = numpy.ones(nface)
        cpsi[ok] = numpy.einsum("ij,ij->i", stg[ok], otg[ok]) / (ns[ok] * no[ok])
        inc = numpy.arccos(numpy.clip(cs, -1.0, 1.0))[vis]
        emi = numpy.arccos(numpy.clip(ce, -1.0, 1.0))[vis]
        psi = numpy.arccos(numpy.clip(cpsi, -1.0, 1.0))[vis]
        w = (ce[vis] * area_m2[vis]) / d_m ** 2
        L = g(T[vis, 0])
        f_smooth = float((L * w).sum())
        per[lab] = (L, w, inc, emi, psi, f_smooth, f_real)
        ph = numpy.degrees(numpy.arccos(numpy.dot(oh, sh)))
        print(f"  {lab:<10}{numpy.linalg.norm(ob):10.1f}{ph:7.2f}"
              f"{f_smooth:12.4e}{f_real:12.4e}{f_real/f_smooth:8.3f}")
    print("\n'need' is the boost roughness must supply to reach the observation.\n")
    print(f"  {'opening':>8}{'coverage':>9}{'rms':>7}" +
          "".join(f"{lab:>10}" for lab, _, _ in EPOCHS) + f"{'chi':>9}")
    best = None
    for op in OPENINGS:
        for cov in COVERAGES:
            ratios = []
            for lab, _, _ in EPOCHS:
                L, w, inc, emi, psi, f_smooth, f_real = per[lab]
                if cov == 0.0:
                    fl = f_smooth
                else:
                    R = numpy.asarray(Crater(cov, op).correction_many(inc, emi, psi))
                    fl = float((L * R * w).sum())
                ratios.append(fl / f_real)
                rows.append(dict(opening_deg=op, coverage=cov, epoch=lab,
                                 rms_slope_deg=rms_slope_deg(cov, op) if cov else 0.0,
                                 flux=fl, flux_real=f_real, sim_over_real=fl / f_real))
            chi = float(numpy.sqrt(numpy.mean((numpy.array(ratios) - 1.0) ** 2)))
            if best is None or chi < best[0]:
                best = (chi, op, cov, list(ratios))
            if cov in (0.0, 0.2, 0.4, 0.6, 0.8, 1.0):
                r = rms_slope_deg(cov, op) if cov else 0.0
                print(f"  {op:8.0f}{cov:9.2f}{r:7.1f}" +
                      "".join(f"{v:10.3f}" for v in ratios) + f"{chi:9.4f}")
    pandas.DataFrame(rows).to_csv(OUT / "roughness_sweep.csv", index=False)
    chi, op, cov, ratios = best
    print(f"\nBEST FIT  opening {op:.0f} deg, coverage {cov:.2f}, "
          f"RMS slope {rms_slope_deg(cov, op) if cov else 0.0:.1f} deg   rms miss {chi:.4f}")
    print(f"  sim/real per epoch: " + "  ".join(f"{v:.3f}" for v in ratios))
    print(f"\nwrote {OUT}/roughness_sweep.csv")


def before_render(sim, _dt):
    i = st["i"]
    if i > n_pre:
        return
    et = ET0 - (n_pre - i) * DT
    sb = numpy.asarray(spice.spkpos("SUN", et, body.frame, "none", "DEIMOS")[0])
    u = sb / numpy.linalg.norm(sb)
    sim.bodies[0].mat[:3, :3] = numpy.eye(3)
    sim.bodies[0].mat[:3, 3] = [0.0, 0.0, 0.0]
    sim.sun.anchor = [0.0, 0.0, 0.0]
    sim.sun.pos = u * 200.0
    sim.sun.look_anchor()
    sim.hud = f"preroll {i}/{n_pre}"


def after_render(sim, _dt):
    i = st["i"]
    if i > n_pre:
        return
    if i == n_pre:
        pandas.DataFrame(T).to_csv(STATE, index=False, encoding="utf-8-sig")
        print(f"saved prerolled state to {STATE}")
        sweep()
        sys.stdout.flush()
        import os
        os._exit(0)
    et = ET0 - (n_pre - i) * DT
    ps = numpy.asarray(spice.spkpos("SUN", et, body.frame, "none", "DEIMOS")[0]) * 1e3
    v = ps[None, :] - pos
    ds = numpy.linalg.norm(v, axis=1)
    cosi = numpy.einsum("ij,ij->i", nrm, v / ds[:, None])
    numpy.maximum(cosi, 0.0, out=cosi)
    fr = sim.facet_shadow(0)
    lit = 1.0 - numpy.asarray(fr, float) if fr is not None else numpy.ones(nface)
    routine.step_surface_newton(
        T, SOLAR_CONSTANT * (1 - prop.albedo) * cosi * lit / (ds / AU) ** 2,
        prop.se, prop.conductivity, twodz, threshold=0.1)
    routine.step_conduction(T, d_nodes, coefs)
    st["i"] += 1
    if st["i"] % 50 == 0 and not QUIET:
        print(f"  preroll {st['i']:,}/{n_pre:,}  surface mean "
              f"{float(T[:, 0].mean()):.2f} K  shadowed {int((lit < 0.5).sum()):,}")
        sys.stdout.flush()


app.before_render = before_render
app.after_render = after_render
app.start()
