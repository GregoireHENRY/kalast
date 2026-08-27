"""Implicit (backward Euler) conduction on a variable-spacing grid.

The explicit scheme is bound by `dt <= h- h+ / (2 D)`, and on a geometric
grid the thinnest surface layer sets that for the whole column: a 1 mm first
layer caps dt near 13 s for Didymos, ~600 steps per rotation where ~100 would
resolve the diurnal wave. Over two solar orbits that is millions of
unnecessary steps.

Backward Euler is unconditionally stable, so dt is chosen for accuracy
instead. The cost is a tridiagonal solve per column per step rather than a
handful of multiplies, which is worth it once the stability limit is the
thing biting.

Discretisation, matching `core::conduction_1d_nonuniform`:

    dT/dt = D d2T/dz2,   d2T/dz2 ~ 2/(h- + h+) [ (T+ - T)/h+ - (T - T-)/h- ]

so with `a = 2 D dt / (h- (h- + h+))` and `c = 2 D dt / (h+ (h- + h+))`:

    -a T[i-1] + (1 + a + c) T[i] - c T[i+1] = T[i]^n

Boundaries: Dirichlet at the surface (row `T[0] = T_surface`) and a zero
gradient at the base (row `T[N] - T[N-1] = 0`), the adiabatic floor the
thermophysical model uses.

**History.** This module previously held a partial port of multiheats that
could not run: `flux_bc_implicit` and `bc_up_implicit` were module-level
functions taking `self` and dereferencing `self.temp` / `self.cond` / `self.dx`
which never existed, `flux_bc_implicit` called `bc_up_implicit` with two
arguments against a seven-argument signature, and no routine actually solved
the system. Nothing in the repository called any of it. It is replaced here
rather than patched; the radiative surface boundary is not yet implemented,
see `step_dirichlet`.
"""

import numpy
import scipy.linalg


def banded_matrix(z, diffusivity, dt):
    """Tridiagonal system in scipy `solve_banded((1, 1), ...)` layout.

    Constant in time while the grid, diffusivity and `dt` are unchanged, so
    build it once outside the time loop.

    `diffusivity` may be a scalar or one value per node.
    """
    z = numpy.asarray(z, dtype=numpy.float64)
    nx = z.size
    dz = numpy.diff(z)
    h_lo = dz[:-1]
    h_hi = dz[1:]
    total = h_lo + h_hi

    d = numpy.broadcast_to(numpy.asarray(diffusivity, dtype=numpy.float64), (nx,))
    d_mid = d[1:-1]

    a = 2.0 * d_mid * dt / (h_lo * total)  # couples to T[i-1]
    c = 2.0 * d_mid * dt / (h_hi * total)  # couples to T[i+1]

    ab = numpy.zeros((3, nx))
    # main diagonal
    ab[1, 1:-1] = 1.0 + a + c
    ab[1, 0] = 1.0  # Dirichlet surface
    ab[1, -1] = 1.0  # base, paired with the -1 below
    # upper diagonal, stored at ab[0, 1:]
    ab[0, 2:] = -c
    ab[0, 1] = 0.0  # surface row has no off-diagonal
    # lower diagonal, stored at ab[2, :-1]
    ab[2, :-2] = -a
    ab[2, -2] = -1.0  # zero-gradient base: T[N] - T[N-1] = 0

    return ab


def step_dirichlet(ab, temperature, surface_temperature):
    """One backward-Euler step with a prescribed surface temperature.

    `ab` comes from `banded_matrix`. Returns the new profile.

    This is the boundary condition the analytical validation uses. The
    radiative surface boundary the thermophysical model needs is non-linear in
    T (it balances absorbed flux against `sigma e T^4` plus conduction), so it
    requires a Newton iteration per step on top of this solve, and is **not
    implemented yet** -- use the explicit path for radiative runs until it is.
    """
    rhs = numpy.asarray(temperature, dtype=numpy.float64).copy()
    rhs[0] = surface_temperature
    rhs[-1] = 0.0  # zero-gradient base
    return scipy.linalg.solve_banded((1, 1), ab, rhs)
