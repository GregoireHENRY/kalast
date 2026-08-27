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

import numpy
from matplotlib import pyplot

import kalast
import kalast.tpm.nonuniform as nonuniform
import kalast.tpm.properties as properties
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
    taken = 0

    t = 0.0
    while t < t_end:
        T[0] = analytical(0.0, t)
        T[1:-1] = step(T, d, *coefs)
        T[-1] = T[-2]
        t += dt
        if taken < N_SNAPSHOTS and t >= snap_times[taken]:
            snaps[taken] = T
            taken += 1

    err = numpy.array(
        [numpy.abs(snaps[i] - analytical(z, snap_times[i])) for i in range(taken)]
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
# Backward Euler is unconditionally stable, so dt follows accuracy rather
# than the thinnest layer. Use spin/100, the diurnal resolution that matters.
dt_imp = P / 100.0


def run_implicit(z, dt, label):
    z = numpy.asarray(z, dtype=numpy.float64)
    ab = implicit.banded_matrix(z, D, dt)
    T = analytical(z, 0.0)

    t_end = N_PERIODS * P
    snap_times = numpy.linspace(t_end - P, t_end, N_SNAPSHOTS, endpoint=False)
    snaps = numpy.zeros((N_SNAPSHOTS, z.size))
    taken = 0

    t = 0.0
    while t < t_end:
        t += dt
        T = implicit.step_dirichlet(ab, T, analytical(0.0, t))
        if taken < N_SNAPSHOTS and t >= snap_times[taken]:
            snaps[taken] = T
            taken += 1

    err = numpy.array(
        [numpy.abs(snaps[i] - analytical(z, snap_times[i])) for i in range(taken)]
    )
    print(
        f"{label:38s} nodes={z.size:4d}  dt={dt:7.2f}s  "
        f"max|err|={err.max():7.3f} K  mean|err|={err.mean():6.3f} K"
    )
    return snaps, err.max()


snaps_imp, err_imp = run_implicit(z_geo, dt_imp, "geometric grid, IMPLICIT")
_, err_imp_same = run_implicit(z_geo, dt_fix, "geometric grid, implicit @ explicit dt")

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
    f"-> implicit at dt={dt_imp:.1f}s (spin/100): {err_imp:.2f} K, "
    f"{dt_imp / dt_fix:.1f}x the explicit timestep\n"
    f"   implicit at the explicit dt={dt_fix:.1f}s: {err_imp_same:.2f} K "
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
