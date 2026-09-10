#!/usr/bin/env python
"""The 1D conduction solvers against the analytical damped thermal wave.

A half-space forced by a sinusoidal surface temperature has a closed-form
solution:

    T(z, t) = Tm + Ta * exp(-z/ls) * sin(z/ls - 2*pi*t/P)

so the wave decays by `1/e` and lags by exactly one radian per thermal skin
depth `ls = sqrt(D P / pi)`. Any conduction scheme worth running a
thermophysical model on has to reproduce that, and nothing else in the suite
touches the numerics at all -- `explicit.py`, `implicit.py` and
`nonuniform.py` are 838 lines with no test between them.

`examples/analytical/sinusoidal.py` has validated this from the start, and
this test is deliberately its assertions rather than a new method: the example
*prints* eight error figures and an order-of-accuracy table, which means a
regression is only caught if somebody runs it and reads it. The example keeps
the plots and the commentary; this keeps the numbers honest in CI.

Three kinds of check, weakest to strongest:

1. **Against the closed form.** Per-configuration error budgets, taken from
   what each configuration actually achieves rather than from round numbers.
2. **Against the physics directly.** Amplitude decay and phase lag measured
   *out of the numerical solution* by a Fourier fit at the forcing frequency,
   compared with `exp(-z/ls)` and `z/ls`. This does not consult the analytic
   formula at all, so it survives an error in the formula itself.
3. **Observed order of accuracy.** Halving the timestep against a
   time-converged reference on the same grid must divide the error by 2 for a
   first-order scheme and 4 for a second-order one. This is the check that
   catches a scheme quietly degrading -- an error budget would not, because on
   this grid the spatial error dominates and hides it.

Pure numpy, no GPU and no window; it is the one physics test that needs
neither.
"""

import sys

import numpy

import kalast
import kalast.tpm.explicit as explicit  # noqa: F401  (imported for parity with the example)
import kalast.tpm.implicit as implicit
import kalast.tpm.nonuniform as nonuniform
import kalast.tpm.properties as properties
import kalast.tpm.routine as routine

# Didymos, so this validates the setup `hera_didymos/tpm.py` actually uses.
PROP = kalast.tpm.properties.DIDYMOS
PROP.compute_conductivity_diffusivity()
D = PROP.diffusivity
P = kalast.entity.DIDYMOS.spin_period
LS = properties.skin_depth_1(D, P)

TM = 300.0
TA = 100.0
ZF = 8.0 * LS  # exp(-8) = 3e-4 of the surface amplitude
N_PERIODS = 4

failures: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def close(name, got, want, tol, unit=""):
    ok = abs(got - want) <= tol
    check(name, ok, f"{got:.6g}{unit} vs {want:.6g}{unit} (tol {tol:.3g})")


def analytical(z, t):
    return TM + TA * numpy.exp(-z / LS) * numpy.sin(z / LS - 2.0 * numpy.pi * t / P)


# --- marching -------------------------------------------------------------


def march_explicit(z, dt, stencil="uniform", collect=False):
    """Forward Euler to `N_PERIODS * P`, Dirichlet-forced at the surface.

    Forcing the surface with the analytic value is what makes this a test of
    the conduction scheme alone rather than of the radiative boundary
    condition, which `examples/analytical/sinusoidal.py` covers separately.
    """
    z = numpy.asarray(z, dtype=numpy.float64)
    d = numpy.full(z.size, D, dtype=numpy.float32)
    if stencil == "uniform":
        coefs = (routine.uniform_coefficients(z, dt),)
        step = kalast.tpm.core.conduction_1d
    else:
        coefs = routine.nonuniform_coefficients(z, dt)
        step = kalast.tpm.core.conduction_1d_nonuniform

    T = analytical(z, 0.0).astype(numpy.float32)
    t_end = N_PERIODS * P
    t = 0.0
    worst = 0.0
    times, profiles = [], []

    while t < t_end:
        T[0] = analytical(0.0, t)
        T[1:-1] = step(T, d, *coefs)
        T[-1] = T[-2]  # adiabatic floor
        t += dt
        # Compared at the time actually reached, never at the time intended: a
        # sample landing dt late is a phase offset of order dt, first order and
        # identical for every scheme, so comparing at the requested time would
        # measure the sampling rather than the solver.
        if t >= t_end - P:
            worst = max(worst, float(numpy.abs(T - analytical(z, t)).max()))
            if collect:
                times.append(t)
                profiles.append(T.astype(numpy.float64).copy())

    if collect:
        return worst, numpy.array(times), numpy.array(profiles)
    return worst


def march_implicit(z, n_steps, scheme):
    """Exactly `n_steps` to `N_PERIODS * P`; returns the final profile."""
    z = numpy.asarray(z, dtype=numpy.float64)
    dt = N_PERIODS * P / n_steps
    solver = implicit.Solver(z, D, dt, scheme=scheme)
    T = analytical(z, 0.0)
    for k in range(n_steps):
        T = solver.step_dirichlet(T, analytical(0.0, (k + 1) * dt))
    return T


def implicit_error(z, dt, scheme):
    z = numpy.asarray(z, dtype=numpy.float64)
    solver = implicit.Solver(z, D, dt, scheme=scheme)
    T = analytical(z, 0.0)
    t_end = N_PERIODS * P
    t = 0.0
    worst = 0.0
    while t < t_end:
        t += dt
        T = solver.step_dirichlet(T, analytical(0.0, t))
        if t >= t_end - P:
            worst = max(worst, float(numpy.abs(T - analytical(z, t)).max()))
    return worst


# --- 1. the skin depth itself --------------------------------------------

# Tolerances are relative and f32-sized, not f64-sized, because these are
# Rust functions and the engine builds with `Float = f32` -- `use_f64` exists
# but is not a default feature and maturin does not enable it. At 1e-12
# absolute this fails by ~1e-9 relative on a 1 cm skin depth, which is the
# single-precision mantissa and not a formula error. See the f32 finding in
# `notes/2026-09-10_code_quality_audit.md`; if that decision is ever revisited
# these can tighten.
F32_REL = 1e-6

close(
    "test_skin_depth_1_is_sqrt_DP_over_pi",
    LS,
    numpy.sqrt(D * P / numpy.pi),
    F32_REL * numpy.sqrt(D * P / numpy.pi),
    " m",
)
close(
    "test_skin_depth_2pi_is_2pi_skin_depths",
    properties.skin_depth_2pi(D, P),
    2.0 * numpy.pi * numpy.sqrt(D * P / numpy.pi),
    F32_REL * 2.0 * numpy.pi * numpy.sqrt(D * P / numpy.pi),
    " m",
)

# --- 2. the wave the solver actually produces ----------------------------
# A 10-node-per-skin-depth uniform grid, which section 3 shows is good to
# 0.4 K, then amplitude and phase pulled out by a Fourier fit at the forcing
# frequency. Nothing here consults `analytical()`, so an error in that formula
# -- or in LS -- cannot hide behind itself.

DZ_FINE = LS / 10.0
Z_FINE = numpy.arange(0.0, ZF + DZ_FINE, DZ_FINE)
DT_FINE = 0.4 * kalast.tpm.core.stability_maxdt(D, DZ_FINE * DZ_FINE)

err_fine, times, profiles = march_explicit(Z_FINE, DT_FINE, collect=True)

# Least squares against [1, cos(wt), sin(wt)], not a bare DFT sum. The
# samples span one period only to within a timestep, and a raw sum assumes
# whole periods: the leftover fraction leaks the constant TM into the
# oscillating component and corrupts the phase. That showed up as a 0.21 rad
# lag error on a solver whose amplitude was already correct to 0.5 %, which
# is a measurement artefact and nothing to do with the conduction.
w = 2.0 * numpy.pi / P
basis = numpy.stack(
    [numpy.ones_like(times), numpy.cos(w * times), numpy.sin(w * times)], axis=1
)
coef, *_ = numpy.linalg.lstsq(basis, profiles, rcond=None)
c_cos, c_sin = coef[1], coef[2]

# T = Ta exp(-θ) sin(θ - wt) expands to c_cos = Ta exp(-θ) sin θ and
# c_sin = -Ta exp(-θ) cos θ, so the amplitude is the hypotenuse and the phase
# θ = atan2(c_cos, -c_sin).
amp = numpy.hypot(c_cos, c_sin)
ang = numpy.unwrap(numpy.arctan2(c_cos, -c_sin))

# Only where the wave is still above the discretisation noise: past ~4 skin
# depths the amplitude is under 2 % of the surface and the fit is measuring
# truncation error, not the wave.
depth = Z_FINE / LS
band = depth <= 4.0

decay_err = numpy.abs(amp[band] / amp[0] - numpy.exp(-depth[band])).max()
lag_err = numpy.abs((ang[band] - ang[0]) - depth[band]).max()

check(
    "test_wave_decays_by_one_over_e_per_skin_depth",
    decay_err < 0.02,
    f"max |A(z)/A(0) - exp(-z/ls)| = {decay_err:.4f} over 0..4 skin depths",
)
check(
    "test_wave_lags_one_radian_per_skin_depth",
    lag_err < 0.05,
    f"max phase-lag error = {lag_err:.4f} rad over 0..4 skin depths",
)

# --- 3. agreement with the closed form, per configuration -----------------
# Budgets are the measured error with headroom, not round numbers. Measured on
# this machine, and printed by `examples/analytical/sinusoidal.py` for anyone
# who wants the picture that goes with them.

close_enough = [
    # (name, measured K, budget K, error)
    ("uniform_grid_10_nodes_per_skin_depth", 0.400, 0.60, err_fine),
]

DZ_COARSE = LS / 4.0
Z_COARSE = numpy.arange(0.0, ZF + DZ_COARSE, DZ_COARSE)
DT_COARSE = 0.4 * kalast.tpm.core.stability_maxdt(D, DZ_COARSE * DZ_COARSE)
close_enough.append(
    (
        "uniform_grid_4_nodes_per_skin_depth",
        2.500,
        3.20,
        march_explicit(Z_COARSE, DT_COARSE),
    )
)

Z_GEO = nonuniform.column(LS, m=10, n=5, b=8)
DT_GEO = 0.4 * routine.nonuniform_max_dt(Z_GEO, D)
err_geo_fix = march_explicit(Z_GEO, DT_GEO, stencil="nonuniform")
close_enough.append(
    ("geometric_grid_variable_stencil", 0.691, 1.00, err_geo_fix)
)

DT_IMP = P / 100.0
for scheme, measured, budget in (
    ("backward-euler", 0.848, 1.20),
    ("crank-nicolson", 0.368, 0.60),
    ("bdf2", 0.392, 0.60),
):
    close_enough.append(
        (
            f"implicit_{scheme.replace('-', '_')}",
            measured,
            budget,
            implicit_error(Z_GEO, DT_IMP, scheme),
        )
    )

for name, measured, budget, got in close_enough:
    check(
        f"test_{name}_matches_the_analytical_wave",
        got <= budget,
        f"max|err| = {got:.3f} K (measured {measured:.3f}, budget {budget:.2f})",
    )

# --- 4. the trap the variable-spacing stencil exists to avoid -------------
# `conduction_1d` applies the equal-spacing second difference whatever the
# grid, so on a geometric grid it solves a subtly different equation. That is
# documented in the example; pinning it here stops the two stencils being
# quietly merged, and stops the geometric grid being pointed at the uniform
# one by accident.

err_geo_uniform_stencil = march_explicit(Z_GEO, DT_GEO, stencil="uniform")
check(
    "test_uniform_stencil_is_much_worse_on_a_geometric_grid",
    err_geo_uniform_stencil > 5.0 * err_geo_fix,
    f"{err_geo_uniform_stencil:.2f} K with the uniform stencil vs "
    f"{err_geo_fix:.2f} K with the variable one",
)

# --- 5. observed order of accuracy ---------------------------------------
# The budgets above cannot see this. On a 16-node geometric grid the spatial
# error is ~0.7 K, and Crank-Nicolson and BDF2 sit under it at every timestep
# worth using -- so a second-order scheme silently dropping to first order
# would still pass section 3. Comparing against a time-converged solution on
# the *same* grid cancels the spatial error and leaves only the clock.

REFERENCE = march_implicit(Z_GEO, 65536, "bdf2")
STEPS = [100, 200, 400, 800]
EXPECTED_ORDER = {"backward-euler": 1.0, "crank-nicolson": 2.0, "bdf2": 2.0}

for scheme in implicit.SCHEMES:
    errs = [
        float(numpy.abs(march_implicit(Z_GEO, n, scheme) - REFERENCE).max())
        for n in STEPS
    ]
    orders = [numpy.log2(errs[i] / errs[i + 1]) for i in range(len(errs) - 1)]
    want = EXPECTED_ORDER[scheme]
    got = float(numpy.mean(orders))
    check(
        f"test_{scheme.replace('-', '_')}_converges_at_order_{want:.0f}",
        abs(got - want) < 0.25,
        f"observed {got:.2f} (expected {want:.0f}); "
        f"per halving {', '.join(f'{o:.2f}' for o in orders)}",
    )

sys.exit(1 if failures else 0)
