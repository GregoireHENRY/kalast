"""Helpers that turn a depth grid into what the conduction solvers want.

The solvers in `kalast.tpm.core` take precomputed coefficients rather than a
grid, because those coefficients are constant across a run and recomputing
them every timestep would dominate the cost. These functions build them once,
and answer the questions that otherwise get derived by hand and got wrong:
what the stable timestep is, and how well a grid resolves the thermal wave.
"""

import numpy

from kalast.tpm import properties as _properties


def uniform_coefficients(z, dt):
    """`dt/dx^2` per interior node, for `core.conduction_1d`.

    Only correct when `z` is equally spaced -- `conduction_1d` applies the
    equal-spacing second difference. Use `nonuniform_coefficients` otherwise;
    see the warning there.
    """
    z = numpy.asarray(z, dtype=numpy.float64)
    dz = numpy.diff(z)
    return (dt / (dz[:-1] * dz[:-1])).astype(numpy.float32)


def nonuniform_coefficients(z, dt):
    """Coefficients for `core.conduction_1d_nonuniform` on any depth grid.

    Returns `(coef_lo, coef_hi)`, one pair per interior node, for the
    variable-spacing second derivative

        d2T/dz2 ~ 2/(h- + h+) * [ (T+ - T)/h+ - (T - T-)/h- ]

    where `h- = z[i] - z[i-1]` and `h+ = z[i+1] - z[i]`. On an equally spaced
    grid both reduce to `dt/dx^2` and the result matches
    `uniform_coefficients` exactly.

    **Why this exists.** `conduction_1d` assumes equal spacing. Feeding it a
    geometric grid -- the practical way to reach the seasonal skin depth in
    tens of nodes rather than thousands -- is silently wrong: validated
    against the analytical damped wave it errs by ~12 K where the uniform
    stencil on a uniform grid errs by 0.3 K. See
    `examples/analytical/sinusoidal.py`.
    """
    z = numpy.asarray(z, dtype=numpy.float64)
    dz = numpy.diff(z)
    h_lo = dz[:-1]
    h_hi = dz[1:]
    total = h_lo + h_hi

    coef_lo = 2.0 * dt / (h_lo * total)
    coef_hi = 2.0 * dt / (h_hi * total)
    return coef_lo.astype(numpy.float32), coef_hi.astype(numpy.float32)


def nonuniform_max_dt(z, diffusivity, s=0.5):
    """Largest explicitly stable timestep on a variable-spacing grid.

    The explicit scheme is stable while `D * dt * (coef_lo + coef_hi) <= 2s`,
    which reduces to `dt <= s * h- * h+ / D`. The tightest node sets the
    limit, so a grid with one thin surface layer costs the whole run a small
    timestep -- that trade is the reason to check this before starting.
    """
    z = numpy.asarray(z, dtype=numpy.float64)
    dz = numpy.diff(z)
    return float(s * numpy.min(dz[:-1] * dz[1:]) / diffusivity)


def resolution_report(z, diffusivity, period, label=""):
    """How well `z` resolves the thermal wave of `period`, as a dict.

    Reports nodes per skin depth near the surface and how many skin depths
    the grid spans, which are the two things that decide whether a run is
    trustworthy. A grid too shallow loses the wave into the bottom boundary;
    too coarse at the surface and the diurnal swing is damped numerically.
    """
    z = numpy.asarray(z, dtype=numpy.float64)
    dz = numpy.diff(z)
    ls1 = _properties.skin_depth_1(diffusivity, period)

    return {
        "label": label,
        "nodes": int(z.size),
        "skin_depth_1": ls1,
        "depth": float(z[-1]),
        "depth_in_skin_depths": float(z[-1] / ls1),
        "first_layer": float(dz[0]),
        "nodes_per_skin_depth": float(ls1 / dz[0]),
        "last_layer": float(dz[-1]),
        "max_dt_stable": nonuniform_max_dt(z, diffusivity),
    }


def print_resolution_report(z, diffusivity, period, label=""):
    r = resolution_report(z, diffusivity, period, label)
    head = f"[{r['label']}] " if r["label"] else ""
    print(
        f"{head}{r['nodes']} nodes, {r['depth']:.4g} m deep "
        f"({r['depth_in_skin_depths']:.1f} skin depths)\n"
        f"  ls1={r['skin_depth_1'] * 100:.3f} cm, first layer "
        f"{r['first_layer'] * 1000:.3f} mm "
        f"({r['nodes_per_skin_depth']:.1f} nodes per skin depth), "
        f"last layer {r['last_layer']:.4g} m\n"
        f"  max stable dt = {r['max_dt_stable']:.2f} s"
    )
    return r
