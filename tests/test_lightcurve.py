#!/usr/bin/env python
"""The disc integral, against light curves that have closed forms.

`src/lightcurve.rs` is the driver the photometry was missing: `scattering`
says how bright a facet is, `shadowing` says how much of it is lit and seen,
and this is the sum over facets that produces the one number a photometer
measures. Both halves were tested before it existed; the sum was not, and the
sum is where a wrong normal, a dropped area or a mirrored rotation hides.

Each check has an answer known independently of this code:

1. **A Lommel-Seeliger body at zero phase is exactly its projected area.**
   At `alpha = 0`, `mu0 = mu`, so `r mu0 mu = w mu / (8 pi)` and the sum
   collapses to `sum A mu` -- the projected area, with every trace of the
   reflectance gone but a constant. So a triaxial ellipsoid rotating equator-on
   has the closed-form curve `sqrt(a^2 sin^2 + b^2 cos^2)` and the amplitude
   `2.5 log10(a/b)`.

   The **amplitude** is tolerance-free and holds at 80 facets as well as at
   5120, because an ellipsoid is a linear map of a sphere and an icosphere's
   projected area is isotropic to ~1e-7. The **curve shape** is not: a
   polyhedron's projected area is its own, not the smooth ellipsoid's, so that
   half is a convergence test. A fixed tolerance there measures the mesh --
   at 1280 facets it is 2.4e-4 and it would have set the bar wherever that
   happened to land.

2. **A Lambert sphere follows the analytic phase function**, again under
   refinement rather than against a fixed tolerance.

3. **Occlusion is a no-op on a convex shape -- and each pass, separately, is
   not on a concave one.** The first alone is the trap this repo keeps
   rediscovering: it passes just as well if the clipping never runs. Testing
   the Sun pass and the observer pass one at a time is what makes it evidence,
   and it is how the first version of this file was found to be exercising
   only one of them.

4. **Helmholtz reciprocity.** Swapping the Sun and the observer cannot change
   the flux, for every law here. On a concave shape it is what catches the two
   clipping passes both being run along one direction.

Needs `res/ico*.obj`, no data paths, no GPU and no window.
"""

import sys

import numpy

from kalast._rs import shadowing as sh
from kalast.lightcurve import Spin, flux, lightcurve
from kalast.scattering import Hapke, LommelSeeligerLambert

failures: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def load_obj(path):
    v, f = [], []
    for line in open(path):
        if line.startswith("v "):
            v.append([float(x) for x in line.split()[1:4]])
        elif line.startswith("f "):
            f.append([int(p.split("/")[0]) - 1 for p in line.split()[1:4]])
    return numpy.array(v, float), numpy.array(f, numpy.uint32)


def icosphere(path):
    v, f = load_obj(path)
    return v / numpy.linalg.norm(v, axis=1)[:, None], f


def cratered_sphere(path, n_craters=20, depth=0.20, width=0.25, seed=3):
    """An icosphere with Gaussian dimples, as `shadow_quantisation.py` uses.

    A convex body self-shadows nowhere, so it cannot tell a working clipper
    from one that never ran. These parameters were chosen by measuring: at
    `alpha = 70 deg` they shadow 2.6 % of the illuminated cross-section, where
    the shallower 6-crater set the first version of this file used shadowed
    *nothing* at the Sun direction it tested and let a dead Sun pass through.
    """
    v, f = icosphere(path)
    rng = numpy.random.default_rng(seed)
    c = rng.normal(size=(n_craters, 3))
    c /= numpy.linalg.norm(c, axis=1)[:, None]
    r = numpy.ones(len(v))
    for ci in c:
        ang = numpy.arccos(numpy.clip(v @ ci, -1.0, 1.0))
        r -= depth * numpy.exp(-0.5 * (ang / width) ** 2)
    return v * r[:, None], f


def as32(v, f):
    return (
        numpy.ascontiguousarray(v, numpy.float32),
        numpy.ascontiguousarray(f, numpy.uint32),
    )


def geometry(v, f):
    a, b, c = v[f[:, 0]], v[f[:, 1]], v[f[:, 2]]
    n = numpy.cross(b - a, c - a)
    area = 0.5 * numpy.linalg.norm(n, axis=1)
    return n / numpy.linalg.norm(n, axis=1)[:, None], area


def mmag(x, ref):
    return -2.5 * numpy.log10(x / ref) * 1000.0


# The winding *is* the normal in `flux_at`, so a mesh wound inward would make
# every check below wrong in the same direction at once. Assert it here rather
# than flipping normals quietly the way the example scripts do.
V3, F3 = icosphere("res/ico3.obj")
N3, _ = geometry(V3, F3)
check(
    "test_res_icospheres_are_wound_outward",
    bool(numpy.all(numpy.sum(N3 * V3[F3].mean(axis=1), axis=1) > 0)),
    "winding-derived normals all point away from the centre",
)

# --- 1. Lommel-Seeliger at zero phase is the projected area ---------------
AXES = (2.0, 1.0, 1.0)
LS = LommelSeeligerLambert(w=0.1, c=1.0)
SPIN = Spin(pole_lon=0.0, pole_lat=numpy.pi / 2, period=1.0)
N_PHASE = 24
T = numpy.arange(N_PHASE) / N_PHASE
PHI = 2.0 * numpy.pi * T
SHAPE = numpy.sqrt(AXES[0] ** 2 * numpy.sin(PHI) ** 2 + AXES[1] ** 2 * numpy.cos(PHI) ** 2)


def ellipsoid_curve(path):
    v, f = icosphere(path)
    e, fe = as32(v * numpy.array(AXES), f)
    return lightcurve(e, fe, SPIN, [1.0, 0.0, 0.0], [1.0, 0.0, 0.0], T, LS,
                      shadowing=False, visibility=False)


curve = ellipsoid_curve("res/ico3.obj")
mag = curve.magnitude()
amplitude = float(mag.max() - mag.min())
expected = 2.5 * numpy.log10(AXES[0] / AXES[1])
check(
    "test_ls_ellipsoid_amplitude_is_2p5_log_a_over_b",
    abs(amplitude - expected) < 1e-4,
    f"{amplitude * 1000:.3f} vs {expected * 1000:.3f} mmag",
)

scatter = []
for p in (2, 3, 4):
    r = numpy.asarray(ellipsoid_curve(f"res/ico{p}.obj").flux, float) / SHAPE
    scatter.append((p, float(r.std() / r.mean())))
check(
    "test_ls_ellipsoid_curve_converges_onto_the_projected_area",
    scatter[0][1] > scatter[1][1] > scatter[2][1] and scatter[2][1] < 1e-4,
    "  ".join(f"ico{p}: {s:.2e}" for p, s in scatter),
)

# The same curve the long way, to pin the *sum* rather than the physics: an
# independent numpy disc integral over the same geometry.
sun_body = numpy.stack(
    [numpy.cos(PHI), -numpy.sin(PHI), numpy.zeros_like(PHI)], axis=1
)
n_e, area_e = geometry(V3 * numpy.array(AXES), F3)
mu_e = numpy.clip(n_e @ sun_body.T, 0.0, None)
by_hand = (0.1 / (8.0 * numpy.pi)) * (area_e[:, None] * mu_e).sum(axis=0)
rel = numpy.abs(numpy.asarray(curve.flux, float) - by_hand) / by_hand
check(
    "test_disc_integral_matches_an_independent_numpy_sum",
    float(rel.max()) < 1e-5,
    f"worst {rel.max():.2e} over {N_PHASE} phases",
)

# --- 2. a Lambert sphere converges onto the analytic phase function -------
# int mu0 mu dA over a unit sphere = (2/3)[sin a + (pi - a) cos a], so a
# Lambert sphere's disc integral is that times albedo/pi.
ALBEDO = 0.1
LAMB = LommelSeeligerLambert(w=ALBEDO, c=0.0)
ALPHAS = numpy.radians([0.0, 30.0, 60.0, 90.0])


def sphere_error(path):
    v, f = as32(*icosphere(path))
    worst = 0.0
    for a in ALPHAS:
        p = flux(v, f, [float(numpy.sin(a)), 0.0, float(numpy.cos(a))], [0.0, 0.0, 1.0],
                 LAMB, shadowing=False, visibility=False)
        want = ALBEDO / numpy.pi * (2.0 / 3.0) * (numpy.sin(a) + (numpy.pi - a) * numpy.cos(a))
        worst = max(worst, abs(p.flux - want) / want)
    return worst


errs = [(p, sphere_error(f"res/ico{p}.obj")) for p in (2, 3, 4)]
check(
    "test_lambert_sphere_converges_onto_the_analytic_phase_function",
    errs[0][1] > errs[1][1] > errs[2][1] and errs[2][1] < 2e-3,
    "  ".join(f"ico{p}: {e:.2e}" for p, e in errs),
)

# --- 3. occlusion is a no-op on a convex shape, and is not on a concave one
ALPHA = numpy.radians(70.0)
OBS = [1.0, 0.0, 0.0]
SUN = [float(numpy.cos(ALPHA)), float(numpy.sin(ALPHA)), 0.0]

SPH, FSPH = as32(*icosphere("res/ico2.obj"))
on = flux(SPH, FSPH, SUN, OBS, LS)
off = flux(SPH, FSPH, SUN, OBS, LS, shadowing=False, visibility=False)
check(
    "test_a_convex_shape_is_unchanged_by_the_clipping_passes",
    abs(on.flux - off.flux) / off.flux < 1e-5 and abs(on.lit_fraction - 1.0) < 1e-5,
    f"{on.flux:.6e} vs {off.flux:.6e}, lit {on.lit_fraction:.6f}",
)

CRAT, FCRAT = as32(*cratered_sphere("res/ico3.obj"))
c_none = flux(CRAT, FCRAT, SUN, OBS, LS, shadowing=False, visibility=False)
c_sun = flux(CRAT, FCRAT, SUN, OBS, LS, shadowing=True, visibility=False)
c_obs = flux(CRAT, FCRAT, SUN, OBS, LS, shadowing=False, visibility=True)
c_both = flux(CRAT, FCRAT, SUN, OBS, LS)
check(
    "test_the_sun_pass_alone_changes_a_concave_shape",
    mmag(c_sun.flux, c_none.flux) > 1.0 and c_sun.lit_fraction < 0.99,
    f"{mmag(c_sun.flux, c_none.flux):.1f} mmag, lit {c_sun.lit_fraction:.4f}",
)
check(
    "test_the_observer_pass_alone_changes_a_concave_shape",
    mmag(c_obs.flux, c_none.flux) > 1.0 and abs(c_obs.lit_fraction - 1.0) < 1e-9,
    f"{mmag(c_obs.flux, c_none.flux):.1f} mmag, lit {c_obs.lit_fraction:.4f}",
)
check(
    "test_both_passes_together_are_fainter_than_either",
    c_both.flux < c_sun.flux and c_both.flux < c_obs.flux,
    f"{mmag(c_both.flux, c_none.flux):.1f} mmag against "
    f"{mmag(c_sun.flux, c_none.flux):.1f} and {mmag(c_obs.flux, c_none.flux):.1f}",
)

# The diagnostic must be the thing it claims to be, not a number that merely
# moves: recompute it from `shadowing.lit_fractions` directly.
lit = numpy.asarray(sh.lit_fractions(CRAT, FCRAT, SUN), float)
n_c, area_c = geometry(numpy.asarray(CRAT, float), F3)
mu0_c = n_c @ numpy.array(SUN)
face = mu0_c > 0.0
want_lit = float(
    (area_c[face] * mu0_c[face] * lit[face]).sum() / (area_c[face] * mu0_c[face]).sum()
)
check(
    "test_lit_fraction_is_the_area_weighted_unshadowed_fraction",
    abs(c_both.lit_fraction - want_lit) < 1e-5,
    f"{c_both.lit_fraction:.6f} vs {want_lit:.6f}",
)

# --- 4. Helmholtz reciprocity --------------------------------------------
# `hapke_rough` is the end-to-end one: reciprocity through a concave shape,
# both clipping passes, *and* the two branches of the roughness correction.
for name, law in (
    ("mix", LS),
    ("hapke", Hapke(w=0.1)),
    ("hapke_rough", Hapke(w=0.1, theta_bar=numpy.radians(30.0))),
):
    f_so = flux(CRAT, FCRAT, SUN, OBS, law).flux
    f_os = flux(CRAT, FCRAT, OBS, SUN, law).flux
    check(
        f"test_reciprocity_under_swapping_sun_and_observer_{name}",
        abs(f_so - f_os) / f_so < 1e-5,
        f"{f_so:.6e} vs {f_os:.6e}",
    )

# --- 5. macroscopic roughness, through the disc integral -----------------
try:
    flux(SPH, FSPH, SUN, OBS, Hapke(w=0.1, theta_bar=1.6))
    check("test_impossible_roughness_raises_from_the_disc_integral", False, "no exception")
except ValueError as exc:
    check(
        "test_impossible_roughness_raises_from_the_disc_integral",
        "theta_bar" in str(exc),
        str(exc)[:48],
    )

# Hapke's roughness is a *bidirectional* correction, so what it does to a
# disc-integrated body is not obvious from the per-facet formula. Two
# properties are, and they are opposite ends of the same mechanism: S = 1 at
# zero azimuth, so at opposition the whole sphere is essentially unaffected;
# and away from it the darkening grows with both theta_bar and phase angle.
SPHERE = numpy.ascontiguousarray(
    numpy.asarray(SPH, float) / numpy.linalg.norm(numpy.asarray(SPH, float), axis=1)[:, None],
    numpy.float32,
)


def sphere_flux(tb_deg, alpha_deg):
    a = numpy.radians(alpha_deg)
    return flux(
        SPHERE, FSPH,
        [float(numpy.sin(a)), 0.0, float(numpy.cos(a))], [0.0, 0.0, 1.0],
        Hapke(w=0.10, b=0.30, c=0.60, b0=1.0, h=0.05, theta_bar=numpy.radians(tb_deg)),
        shadowing=False, visibility=False,
    ).flux


at_opposition = mmag(sphere_flux(30.0, 0.0), sphere_flux(0.0, 0.0))
check(
    "test_roughness_barely_touches_a_sphere_at_opposition",
    abs(at_opposition) < 2.0,
    f"{at_opposition:.2f} mmag at theta_bar = 30 deg",
)

rows = [(a, mmag(sphere_flux(30.0, a), sphere_flux(0.0, a))) for a in (0, 20, 60, 100)]
check(
    "test_roughness_darkens_a_sphere_more_at_larger_phase_angle",
    all(x[1] < y[1] for x, y in zip(rows, rows[1:])) and rows[-1][1] > 300.0,
    "  ".join(f"{a:d} deg: {d:.0f}" for a, d in rows) + " mmag",
)

# And the consequence that matters for shape inversion: at an ordinary
# observing geometry, roughness changes the *amplitude* of the rotation curve,
# not just its level -- so it is not divided out by normalising, and it maps
# straight onto a fitted axis ratio. Measured at 15 % for theta_bar = 30 deg,
# which is far above the noise of ground-based relative photometry.
amp = {}
for tb in (0.0, 30.0):
    m = lightcurve(
        numpy.ascontiguousarray(V3 * numpy.array(AXES), numpy.float32), F3, SPIN,
        [float(numpy.cos(numpy.radians(20.0))), float(numpy.sin(numpy.radians(20.0))), 0.0],
        [1.0, 0.0, 0.0], T,
        Hapke(w=0.10, b=0.30, c=0.60, b0=1.0, h=0.05, theta_bar=numpy.radians(tb)),
        shadowing=False, visibility=False,
    ).magnitude()
    amp[tb] = float(m.max() - m.min()) * 1000.0
check(
    "test_roughness_changes_the_rotation_curve_amplitude",
    (amp[30.0] - amp[0.0]) / amp[0.0] > 0.08,
    f"{amp[0.0]:.1f} -> {amp[30.0]:.1f} mmag, "
    f"{(amp[30.0] - amp[0.0]) / amp[0.0] * 100:.0f} % at theta_bar = 30 deg",
)

# --- 6. the spin state ----------------------------------------------------
for lon, lat in ((0.0, 1.2), (2.5, -0.7), (-1.0, 0.0)):
    s = Spin(pole_lon=lon, pole_lat=lat)
    want = [numpy.cos(lat) * numpy.cos(lon), numpy.cos(lat) * numpy.sin(lon), numpy.sin(lat)]
    check(
        f"test_pole_vector_lon{lon}_lat{lat}",
        float(numpy.abs(numpy.asarray(s.pole()) - want).max()) < 1e-6,
    )
s = Spin(period=5.0, phase0=0.25, epoch0=2.0)
check(
    "test_phase_advances_one_turn_per_period",
    abs(s.phase_at(2.0) - 0.25) < 1e-6 and abs(s.phase_at(7.0) - (0.25 + 2 * numpy.pi)) < 1e-4,
    f"{s.phase_at(2.0):.4f} -> {s.phase_at(7.0):.4f}",
)

# --- 7. magnitudes --------------------------------------------------------
f_arr = numpy.asarray(curve.flux, float)
med = float(numpy.median(f_arr))
check(
    "test_magnitude_defaults_to_the_median_flux",
    float(numpy.abs(curve.magnitude() - (-2.5 * numpy.log10(f_arr / med))).max()) < 1e-5,
)
check(
    "test_magnitude_takes_an_explicit_reference",
    float(numpy.abs(curve.magnitude(1.0) - (-2.5 * numpy.log10(f_arr))).max()) < 1e-5,
)
check("test_curve_reports_no_overflow", curve.overflowed is False)

sys.exit(1 if failures else 0)
