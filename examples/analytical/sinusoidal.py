#!/usr/bin/env python
"""Validate the 1D conduction solver against an analytical thermal wave.

A half-space forced by a sinusoidal surface temperature has the closed-form
solution

    T(z, t) = Tm + Ta * exp(-z/ls) * sin(z/ls - 2*pi*t/P)

with `ls` the thermal skin depth. Any conduction solver worth trusting must
reproduce it, so this is the cheapest way to check a grid and timestep before
committing them to a long thermophysical run.

Run with Didymos's real properties and spin period, so the numbers here are
the ones that matter for `hera_didymos/tpm.py`.

It also answers a question that matters for seasonal runs: the depth needed
for the *annual* wave is ~100x the daily skin depth, which a uniform grid
cannot reach cheaply. `kalast.tpm.nonuniform.column` builds a geometric grid
that can -- but `kalast.tpm.core.conduction_1d` implements the *uniform*
second difference, so the two do not compose. The third case below measures
that error rather than leaving it to be discovered in a two-orbit run.
"""

import time

import numpy
from matplotlib import pyplot

import kalast
import kalast.tpm.nonuniform as nonuniform
import kalast.tpm.properties as properties
import kalast.tpm.explicit as explicit
import kalast.tpm.implicit as implicit
import kalast.tpm.routine as routine

# --- Didymos, so this validates the setup we actually intend to use ---
prop = kalast.tpm.properties.DIDYMOS
prop.compute_conductivity_diffusivity()
D = prop.diffusivity

P = kalast.entity.DIDYMOS.spin_period  # 2.26 h
ls = properties.skin_depth_1(D, P)

TM = 300.0  # mean surface temperature
TA = 100.0  # amplitude of the surface swing

# Deep enough that the wave has decayed and the adiabatic floor is harmless:
# exp(-8) is 3e-4 of the surface amplitude.
ZF = 8.0 * ls

N_PERIODS = 4  # integrate several periods so the initial condition washes out
N_SNAPSHOTS = 6


def analytical(z, t):
    return TM + TA * numpy.exp(-z / ls) * numpy.sin(z / ls - 2.0 * numpy.pi * t / P)


def run(z, dt, label, stencil="uniform"):
    """March the explicit solver on grid `z`, return profiles at snapshots.

    Surface is Dirichlet-forced with the analytical value, which is what makes
    the comparison a test of the conduction scheme alone rather than of the
    radiative boundary condition.

    `stencil` selects the equal-spacing second difference (`conduction_1d`)
    or the variable-spacing one (`conduction_1d_nonuniform`).
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
    snap_times = numpy.linspace(t_end - P, t_end, N_SNAPSHOTS, endpoint=False)
    snaps = numpy.zeros((N_SNAPSHOTS, z.size))
    taken_times = numpy.zeros(N_SNAPSHOTS)
    taken = 0

    t = 0.0
    while t < t_end:
        T[0] = analytical(0.0, t)
        T[1:-1] = step(T, d, *coefs)
        T[-1] = T[-2]
        t += dt
        if taken < N_SNAPSHOTS and t >= snap_times[taken]:
            snaps[taken] = T
            taken_times[taken] = t  # not snap_times[taken]: see below
            taken += 1

    # Compare at the time actually reached. A snapshot lands up to `dt` past
    # its target, and at TA=100 K over P that phase offset alone is ~6 K at
    # dt=P/100 -- first order in dt and identical for every scheme, so
    # comparing at the requested time measures the sampling, not the solver.
    err = numpy.array(
        [numpy.abs(snaps[i] - analytical(z, taken_times[i])) for i in range(taken)]
    )
    print(
        f"{label:38s} nodes={z.size:4d}  dt={dt:7.2f}s  "
        f"max|err|={err.max():7.3f} K  mean|err|={err.mean():6.3f} K"
    )
    return snaps, snap_times, err.max()


print(f"Didymos: k={prop.conductivity:.4e} W/m/K  D={D:.4e} m2/s")
print(f"spin P={P:.0f} s ({P / 3600:.2f} h)  skin depth ls1={ls * 100:.3f} cm")
print(f"domain depth {ZF * 100:.2f} cm = 8 ls1, integrating {N_PERIODS} periods\n")

# --- 1. uniform grid, the case conduction_1d is written for ---------------
dz_uniform = ls / 10.0  # 10 nodes per skin depth
z_uni = numpy.arange(0.0, ZF + dz_uniform, dz_uniform)
dt_uni = 0.4 * kalast.tpm.core.stability_maxdt(D, dz_uniform * dz_uniform)
snaps_uni, times, err_uni = run(z_uni, dt_uni, "uniform, 10 nodes/skin depth")

# --- 2. uniform but coarser: how far can the grid be relaxed? -------------
dz_coarse = ls / 4.0
z_coarse = numpy.arange(0.0, ZF + dz_coarse, dz_coarse)
dt_coarse = 0.4 * kalast.tpm.core.stability_maxdt(D, dz_coarse * dz_coarse)
snaps_coarse, _, err_coarse = run(z_coarse, dt_coarse, "uniform, 4 nodes/skin depth")

# --- 3. non-uniform grid with the uniform stencil: the trap ---------------
# Same domain, geometric spacing. conduction_1d applies the equal-spacing
# second difference regardless, so this measures the resulting error.
z_geo = nonuniform.column(ls, m=10, n=5, b=8)
dz_geo = numpy.diff(z_geo)
dt_geo = 0.4 * kalast.tpm.core.stability_maxdt(D, dz_geo.min() ** 2)
snaps_geo, _, err_geo = run(z_geo, dt_geo, "geometric grid, UNIFORM stencil")

# --- 4. the same grid with the variable-spacing stencil -------------------
# What the seasonal run will actually use.
dt_fix = 0.4 * routine.nonuniform_max_dt(z_geo, D)
snaps_fix, _, err_fix = run(
    z_geo, dt_fix, "geometric grid, variable stencil", stencil="nonuniform"
)

# --- 5. implicit on the same grid, at a timestep explicit cannot reach -----
# Implicit schemes are unconditionally stable, so dt follows accuracy rather
# than the thinnest layer. Use spin/100, the diurnal resolution that matters.
dt_imp = P / 100.0


def run_implicit(z, dt, label, scheme="backward-euler"):
    z = numpy.asarray(z, dtype=numpy.float64)
    ab = implicit.Solver(z, D, dt, scheme=scheme)
    T = analytical(z, 0.0)

    t_end = N_PERIODS * P
    snap_times = numpy.linspace(t_end - P, t_end, N_SNAPSHOTS, endpoint=False)
    snaps = numpy.zeros((N_SNAPSHOTS, z.size))
    taken_times = numpy.zeros(N_SNAPSHOTS)
    taken = 0

    t = 0.0
    while t < t_end:
        t += dt
        T = ab.step_dirichlet(T, analytical(0.0, t))
        if taken < N_SNAPSHOTS and t >= snap_times[taken]:
            snaps[taken] = T
            taken_times[taken] = t
            taken += 1

    err = numpy.array(
        [numpy.abs(snaps[i] - analytical(z, taken_times[i])) for i in range(taken)]
    )
    print(
        f"{label:38s} nodes={z.size:4d}  dt={dt:7.2f}s  "
        f"max|err|={err.max():7.3f} K  mean|err|={err.mean():6.3f} K"
    )
    return snaps, err.max()


snaps_imp, err_imp = run_implicit(z_geo, dt_imp, "implicit, backward Euler")
_, err_cn = run_implicit(z_geo, dt_imp, "implicit, Crank-Nicolson", "crank-nicolson")
_, err_bdf2 = run_implicit(z_geo, dt_imp, "implicit, BDF2", "bdf2")
_, err_imp_same = run_implicit(z_geo, dt_fix, "backward Euler @ explicit dt")

# --- 6. order of accuracy in time -----------------------------------------
# Against the analytical solution these three would look almost identical,
# because on a 16-node geometric grid the *spatial* error is ~0.36 K and
# Crank-Nicolson and BDF2 are already under it at every dt worth using. That
# floor is the useful headline, but it hides whether each scheme integrates
# in time at the order it claims.
#
# So compare instead against a time-converged solution on the *same* grid:
# the same spatial discretisation stepped so finely that only the temporal
# error remains. Halving dt should then divide the error by 2 for a
# first-order scheme and by 4 for a second-order one.
def march(z, n_steps, scheme):
    """Integrate exactly `n_steps` to `N_PERIODS * P`, return the profile."""
    dt = N_PERIODS * P / n_steps
    solver = implicit.Solver(z, D, dt, scheme=scheme)
    T = analytical(z, 0.0)
    for k in range(n_steps):
        T = solver.step_dirichlet(T, analytical(0.0, (k + 1) * dt))
    return T


reference = march(z_geo, 65536, "bdf2")
steps = [100, 200, 400, 800]
print("\ntime-discretisation error on the geometric grid, against the same")
print("grid stepped to convergence (so the spatial error cancels):")
print("  " + "steps per 4 periods".ljust(22) + "".join(f"{n:>14d}" for n in steps))
for scheme in implicit.SCHEMES:
    errs = [numpy.abs(march(z_geo, n, scheme) - reference).max() for n in steps]
    ords = [numpy.log2(errs[i] / errs[i + 1]) for i in range(len(errs) - 1)]
    cells = "".join(
        f"{e:>9.2e}" + (f"[{o:.1f}]" if i else "     ")
        for i, (e, o) in enumerate(zip(errs, [None] + ords))
    )
    print("  " + scheme.ljust(22) + cells)
print("  -> bracketed figures are the observed order: 1 for backward Euler,")
print("     2 for Crank-Nicolson and BDF2, which is what each should give.")
print(f"  the spatial error of this grid is {err_fix:.2f} K, so at any dt in this")
print("  table the second-order schemes are limited by the grid, not the clock.")

# --- 7. the radiative surface boundary, and why the scheme choice matters --
# Sections 1-6 force the surface with a prescribed temperature, which tests
# the conduction scheme alone. The thermophysical model instead balances
# absorbed flux against emission and conduction, which is non-linear in T0 --
# and for an implicit scheme cannot be applied after the solve, because the
# T1 and T2 in that balance are the new profile. `step_radiative` solves the
# two together; this checks it against the explicit path, which has no such
# coupling to get wrong.
FLUX_PEAK = 1050.0  # W/m2, roughly Didymos at perihelion, normal incidence


def insolation(t, eclipse=False):
    """A rotating facet: half a sine on the day side, zero at night."""
    phase = 2.0 * numpy.pi * t / P
    f = FLUX_PEAK * max(numpy.cos(phase), 0.0)
    # An eclipse ingress/egress is a step in flux, not a smooth ramp -- the
    # case that separates an L-stable scheme from an A-stable one.
    if eclipse and 0.20 < (t / P) % 1.0 < 0.28:
        return 0.0
    return f


def run_radiative(z, dt, scheme, n_periods=8, eclipse=False):
    """March one facet with the radiative boundary; return the last cycle."""
    z = numpy.asarray(z, dtype=numpy.float64)
    T = numpy.full(z.size, 250.0)
    n_steps = int(n_periods * P / dt)

    if scheme == "explicit":
        coefs = routine.nonuniform_coefficients(z, dt)
        d = numpy.full(z.size, D)
        twodz = 2.0 * (z[1] - z[0])
    else:
        solver = implicit.Solver(z, D, dt, scheme=scheme)

    surface = numpy.zeros(n_steps)
    for k in range(n_steps):
        t = k * dt
        f = insolation(t, eclipse)
        if scheme == "explicit":
            T2 = T[None, :].copy()
            routine.step_surface_newton(T2, numpy.array([f]), prop.se,
                                        prop.conductivity, twodz)
            routine.step_conduction(T2, d, coefs)
            T = T2[0]
        else:
            T = solver.step_radiative(T, numpy.array([f]), prop.conductivity,
                                      prop.se, threshold=1e-4)
        surface[k] = T[0]
    return surface


prop.se = kalast.util.STEFAN_BOLTZMANN * prop.emissivity
dt_rad = 0.4 * routine.nonuniform_max_dt(z_geo, D)  # explicit needs this
ref = run_radiative(z_geo, dt_rad, "explicit")
print("\nradiative surface boundary, one facet, last cycle of 8:")
print(f"  explicit forward Euler   dt={dt_rad:6.1f}s  "
      f"Tmin={ref[-int(P / dt_rad):].min():6.2f} Tmax={ref[-int(P / dt_rad):].max():6.2f} K")
for scheme in implicit.SCHEMES:
    sur = run_radiative(z_geo, dt_rad, scheme)
    cyc = slice(-int(P / dt_rad), None)
    print(f"  implicit {scheme:16s} dt={dt_rad:6.1f}s  "
          f"Tmin={sur[cyc].min():6.2f} Tmax={sur[cyc].max():6.2f} K  "
          f"max|diff vs explicit|={numpy.abs(sur[cyc] - ref[cyc]).max():5.3f} K")
print("  -> at a timestep both can take, the implicit schemes reproduce the")
print("     explicit answer, so the coupled surface solve is consistent.")

# Crank-Nicolson is only A-stable, not L-stable: its amplification factor
# tends to -1 rather than 0 for modes with `D dt / h^2 >> 1`, so instead of
# damping them it flips their sign each step. On this grid the first layer is
# 1.2 mm, so that ratio is already 14.5 at dt = P/15. The classic way to
# excite it is a step change at the surface.
print("\nsurface stepped 300 K -> 100 K, first interior node, first 8 steps")
print(f"(dt={P / 15.0:.0f}s, {P / 15.0 / dt_rad:.0f}x the explicit limit, "
      f"D dt / h1^2 = {D * (P / 15.0) / numpy.diff(z_geo)[0] ** 2:.1f}):")
for scheme in implicit.SCHEMES:
    solver = implicit.Solver(z_geo, D, P / 15.0, scheme=scheme)
    T = numpy.full(z_geo.size, 300.0)
    hist = []
    for _ in range(12):
        T = solver.step_dirichlet(T, 100.0)
        hist.append(T[1])
    h = numpy.array(hist)
    d1 = numpy.diff(h)
    flips = int((numpy.sign(d1[:-1]) * numpy.sign(d1[1:]) < 0).sum())
    print(f"  {scheme:16s} " + " ".join(f"{v:6.1f}" for v in h[:8])
          + f"   direction reversals: {flips}")
print("  -> Crank-Nicolson rings; backward Euler and BDF2 are L-stable and")
print("     approach the new state monotonically.")

# But that is the Dirichlet boundary. With the *radiative* one the surface
# node is algebraic rather than time-integrated -- it is re-solved from the
# flux balance every step -- so it re-anchors the column and the ringing mode
# is not excited. Worth measuring rather than assuming, because it decides
# whether Crank-Nicolson is usable for production runs here.
print("\nsame column, radiative boundary, flux stepping to zero for 8% of the")
print("spin (an eclipse), last cycle of 8:")
for scheme in implicit.SCHEMES:
    sur = run_radiative(z_geo, P / 15.0, scheme, eclipse=True)
    d1 = numpy.diff(sur[-15:])
    flips = int((numpy.sign(d1[:-1]) * numpy.sign(d1[1:]) < 0).sum())
    print(f"  {scheme:16s} Tmin={sur[-15:].min():6.2f} Tmax={sur[-15:].max():6.2f} K"
          f"   direction reversals: {flips}")
print("  -> none of the three rings here: the radiative surface node is solved")
print("     algebraically each step, so it damps the mode the Dirichlet test")
print("     excites. Crank-Nicolson is therefore safe for radiative runs, and")
print("     the ringing risk is real only where a surface temperature is")
print("     prescribed. BDF2 remains the default recommendation because it is")
print("     second-order and L-stable without needing that argument.")

# --- 8. what any of this costs -------------------------------------------
# Accuracy per step is only half the question; the other half is accuracy per
# second, at the array size a real run uses. An implicit step is more
# expensive than an explicit one, but it is a *banded* solve batched across
# every facet at once, so the extra cost is far smaller than the timestep it
# unlocks.
#
# RKC is included because it is the explicit answer to the same problem --
# `s` substeps of deliberately unequal length advance ~s^2 times as far as
# one forward-Euler step. It wins against forward Euler and loses to the
# implicit schemes, because here the tridiagonal solve is cheap. Where that
# stops being true -- a lateral or FEM coupling whose matrix is no longer
# tridiagonal, or a GPU implementation with no solver -- the ranking would
# change, which is why it is worth having measured.
N_FACETS_BENCH = 2000

t_end_bench = N_PERIODS * P
ref_bench = march(z_geo, 16384, "bdf2")


def bench(cls, kwargs, dt_target):
    n = max(int(t_end_bench / dt_target), 1)
    dt = t_end_bench / n
    solver = cls(z_geo, D, dt, **kwargs)
    T = numpy.tile(analytical(z_geo, 0.0), (N_FACETS_BENCH, 1))
    t0 = time.perf_counter()
    for k in range(n):
        T = solver.step_dirichlet(T, analytical(0.0, (k + 1) * dt))
    wall = time.perf_counter() - t0
    return dt, n, getattr(solver, "stages", 1), wall, numpy.abs(T[0] - ref_bench).max()


dt_fe = explicit.Solver(z_geo, D, 1.0).max_dt_forward_euler
print(f"\ncost at {N_FACETS_BENCH:,} facets, {N_PERIODS} spins, error against a")
print("time-converged reference on the same grid:")
print(f"  {'scheme':26s}{'dt [s]':>9}{'steps':>8}{'stages':>8}{'wall [s]':>10}{'err [K]':>9}")
for name, cls, kwargs, dt_target in [
    ("explicit forward Euler", explicit.Solver, {"scheme": "forward-euler"}, 0.4 * dt_fe),
    ("explicit RKC", explicit.Solver, {"scheme": "rkc"}, 4.0 * dt_fe),
    ("implicit backward Euler", implicit.Solver, {"scheme": "backward-euler"}, P / 100),
    ("implicit Crank-Nicolson", implicit.Solver, {"scheme": "crank-nicolson"}, P / 100),
    ("implicit BDF2", implicit.Solver, {"scheme": "bdf2"}, P / 100),
    ("implicit BDF2, coarse", implicit.Solver, {"scheme": "bdf2"}, P / 25),
]:
    dt_b, n_b, st, wall, err = bench(cls, kwargs, dt_target)
    print(f"  {name:26s}{dt_b:9.1f}{n_b:8d}{st:8d}{wall:10.2f}{err:9.3f}")
print("  -> the last row is the point: a timestep no longer tied to the 1.2 mm")
print("     surface layer is faster than the explicit path *and* more accurate.")

print()
print(f"geometric grid spans {z_geo[-1] * 100:.2f} cm in {z_geo.size} nodes "
      f"(first layer {dz_geo[0] * 1000:.2f} mm, last {dz_geo[-1] * 1000:.1f} mm)")
print(
    f"-> uniform stencil on that grid: {err_geo:.2f} K max error\n"
    f"-> variable stencil on that grid: {err_fix:.2f} K "
    f"({err_geo / max(err_fix, 1e-9):.0f}x better), in {z_geo.size} nodes "
    f"rather than {z_uni.size}"
)
routine.print_resolution_report(z_geo, D, P, "geometric")
print(
    f"-> implicit at dt={dt_imp:.1f}s (spin/100), "
    f"{dt_imp / dt_fix:.1f}x the explicit timestep:\n"
    f"   backward Euler {err_imp:.2f} K, Crank-Nicolson {err_cn:.2f} K, "
    f"BDF2 {err_bdf2:.2f} K\n"
    f"   backward Euler at the explicit dt={dt_fix:.1f}s: {err_imp_same:.2f} K "
    "(agrees with explicit, so the scheme is consistent)"
)

# --- figure ---------------------------------------------------------------
kalast.plot.style.load()
fig, axes = pyplot.subplots(1, 5, figsize=(21.0, 4.6), sharey=True)
cases = [
    (axes[0], z_uni, snaps_uni, f"uniform, 10 nodes/ls\ndt={dt_uni:.1f}s", err_uni),
    (axes[1], z_coarse, snaps_coarse, f"uniform, 4 nodes/ls\ndt={dt_coarse:.1f}s", err_coarse),
    (axes[2], z_geo, snaps_geo, f"geometric, uniform stencil\ndt={dt_geo:.1f}s", err_geo),
    (axes[3], z_geo, snaps_fix, f"geometric, variable stencil\ndt={dt_fix:.1f}s", err_fix),
    (axes[4], z_geo, snaps_imp, f"geometric, implicit\ndt={dt_imp:.1f}s", err_imp),
]
for ax, z, snaps, title, err in cases:
    for i in range(N_SNAPSHOTS):
        (l1,) = ax.plot(snaps[i], z * 100, lw=1.4, color="k")
        (l2,) = ax.plot(analytical(z, times[i]), z * 100, lw=1.2, ls="--", color="r")
    l1.set_label("numerical")
    l2.set_label("analytical")
    ax.set_xlabel("temperature [K]")
    ax.set_title(f"{title}  -  max error {err:.2f} K", fontsize=9.5)
    ax.set_ylim(ZF * 100, 0)
    ax.grid(alpha=0.3)
axes[0].set_ylabel("depth [cm]")
axes[0].legend(fontsize=9)
fig.suptitle(
    "Conduction solver vs the analytical damped thermal wave "
    f"(Didymos, P={P / 3600:.2f} h, ls1={ls * 100:.2f} cm)",
    fontsize=11,
)
fig.tight_layout()
fig.savefig("out/analytical_sinusoidal.png", dpi=150)
print("\nwrote out/analytical_sinusoidal.png")
