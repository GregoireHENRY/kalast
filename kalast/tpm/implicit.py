"""Implicit conduction on a variable-spacing grid: three schemes, one solver.

The explicit scheme is bound by `dt <= h- h+ / (2 D)`, and on a geometric
grid the thinnest surface layer sets that for the whole column: a 1 mm first
layer caps dt near 13 s for Didymos, ~600 steps per rotation where ~100 would
resolve the diurnal wave. Over two solar orbits that is millions of
unnecessary steps.

Implicit schemes are unconditionally stable, so dt is chosen for accuracy
instead. The cost is a banded solve per step rather than a handful of
multiplies, which is worth it once the stability limit is the thing biting.

Schemes
-------

All three discretise the interior identically -- the variable-spacing second
difference of `core::conduction_1d_nonuniform` -- and differ only in how they
step it in time. `Solver(scheme=...)` selects between:

``backward-euler``
    First order in time, L-stable. The stiff modes it cannot resolve are
    damped to nothing in one step, which is exactly what you want on a
    geometric grid where the surface layers carry modes 1000x faster than
    anything physical. The safe default, and the reason it is the default.

``crank-nicolson``
    Second order in time (the theta = 1/2 midpoint of the same family), and
    the most accurate of the three at a given dt -- by a factor of about four
    over BDF2, whose error constant is larger. It is only A-stable, not
    L-stable: its amplification factor tends to -1 rather than 0 for stiff
    modes, so a mode with `D dt / h^2 >> 1` is not damped but *flipped in
    sign* each step, giving a decaying oscillation ("ringing") after a sharp
    change.

    That risk is real but narrower than it sounds here, and
    `examples/analytical/sinusoidal.py` measures which side of it a run falls
    on. Step the *prescribed* surface temperature and Crank-Nicolson rings
    plainly where the other two do not. Step the *flux* instead -- an eclipse
    ingress, the case one would actually worry about -- and it does not ring
    at all, because the radiative surface node is algebraic rather than
    time-integrated: it is re-solved from the flux balance at every step, and
    that re-anchoring damps the mode. So Crank-Nicolson is safe for radiative
    runs, and the warning applies to Dirichlet-forced ones.

``bdf2``
    Second order *and* L-stable, which is the combination Crank-Nicolson
    cannot offer: accuracy without the ringing. The cost is that it is a
    two-step method -- it needs the profile from two levels back, so it
    cannot start itself. The solver bootstraps with one backward-Euler step,
    which costs one order locally and nothing asymptotically. It also assumes
    a fixed dt; change dt and the stored history is discarded and the
    bootstrap repeats.

Rule of thumb: `bdf2` for production -- second-order and L-stable, so it
needs no argument about which boundary condition is in use. `crank-nicolson`
when accuracy at a given dt is what matters and the surface is flux-driven.
`backward-euler` when something is behaving strangely and the most forgiving
scheme is wanted.

Measured on the analytical wave, 4 spins of Didymos at 10,000 facets on a
16-node geometric grid, against a time-converged reference:

    scheme                    dt [s]   steps   wall [s]   max err [K]
    explicit forward Euler       9.0    3636       2.24         0.588
    explicit RKC (3 stages)     89.7     363       0.73         4.489
    implicit backward Euler     81.4     400       0.41         0.805
    implicit Crank-Nicolson     81.4     400       0.63         0.006
    implicit BDF2               81.4     400       0.46         0.023
    implicit BDF2              325.4     100       0.12         0.393

The last row is the point of the module: 19x faster than the explicit path
and still more accurate, because the timestep no longer has to respect a
1.2 mm surface layer.

Boundary conditions
-------------------

`step_dirichlet` prescribes the surface temperature -- what the analytical
validation uses. `step_radiative` solves the real thermophysical boundary,

    F - sigma eps T0^4 + k (-3 T0 + 4 T1 - T2) / (2 dz) = 0

which is non-linear in T0 and, unlike the explicit path, cannot be applied
after the fact: T1 and T2 in that expression are the *new* profile, which
depends on T0 through the solve. Both are handled by the same decomposition.

Only the surface row is non-linear, and only the surface node couples into
the interior system, so the interior can be solved once for the two states it
can be in:

    interior = U + T0 * V

where `U` solves the interior system with the surface clamped to zero and `V`
is its response to a unit surface temperature. `V` depends only on the grid,
so it is computed once in the constructor; `U` is one banded solve per step,
batched across every facet at once. The surface then reduces to a *scalar*
Newton iteration per facet on

    R(T0) = F + H - se T0^4 + G T0,    G, H from U and V,

with `G` a constant and `H` per-facet. That is a few vectorised array
operations, not 10,000 separate linear solves, and it is exact rather than a
lagged-coefficient approximation: the surface and interior are converged
together at the new time level.

The base is a zero-gradient (adiabatic) floor throughout, matching the
explicit path.

History
-------

This module previously held a partial port of multiheats that could not run:
`flux_bc_implicit` and `bc_up_implicit` were module-level functions taking
`self` and dereferencing `self.temp` / `self.cond` / `self.dx` which never
existed, `flux_bc_implicit` called `bc_up_implicit` with two arguments
against a seven-argument signature, and no routine actually solved the
system. Nothing in the repository called any of it.
"""

import numpy
import scipy.linalg

SCHEMES = ("backward-euler", "crank-nicolson", "bdf2")

#: Implicit weight on the new time level. `bdf2` is not a member of the theta
#: family; its effective weight is 2/3 with a three-level right-hand side.
_THETA = {"backward-euler": 1.0, "crank-nicolson": 0.5, "bdf2": 2.0 / 3.0}


class Solver:
    """Implicit conduction for a column, or for every facet's column at once.

    Parameters
    ----------
    z : (nx,) array
        Node depths, increasing. Spacing may be arbitrary.
    diffusivity : float or (nx,) array
        Thermal diffusivity, scalar or per node.
    dt : float
        Timestep. Unconditionally stable, so choose it for accuracy: a few
        hundred steps per rotation resolves the diurnal wave.
    scheme : str
        One of `SCHEMES`.

    Notes
    -----
    The matrix depends only on `(z, diffusivity, dt, scheme)`, so one solver
    serves a whole run and every facet. Temperature arrays may be `(nx,)` for
    a single column or `(n_facets, nx)`; the shape passed in is the shape
    returned.
    """

    def __init__(self, z, diffusivity, dt, scheme="backward-euler"):
        if scheme not in SCHEMES:
            raise ValueError(f"scheme must be one of {SCHEMES}, got {scheme!r}")

        z = numpy.asarray(z, dtype=numpy.float64)
        if z.ndim != 1 or z.size < 4:
            raise ValueError("z must be 1-D with at least 4 nodes")

        self.z = z
        self.dt = float(dt)
        self.scheme = scheme
        self.nx = nx = z.size
        self.theta = _THETA[scheme]

        dz = numpy.diff(z)
        d = numpy.broadcast_to(
            numpy.asarray(diffusivity, dtype=numpy.float64), (nx,)
        )

        # Variable-spacing second difference, without dt, valid on 1..nx-2.
        h_lo, h_hi = dz[:-1], dz[1:]
        total = h_lo + h_hi
        self.a = numpy.zeros(nx)
        self.c = numpy.zeros(nx)
        self.a[1:-1] = 2.0 * d[1:-1] / (h_lo * total)
        self.c[1:-1] = 2.0 * d[1:-1] / (h_hi * total)

        # Weight on the new level. For bdf2 the scheme is
        # (3T^{n+1} - 4T^n + T^{n-1})/(2 dt) = D L(T^{n+1}), i.e. the same
        # matrix as backward Euler at 2/3 dt with a two-level right-hand side.
        alpha = self.theta * self.dt
        self._a_new = alpha * self.a
        self._c_new = alpha * self.c
        # Explicit share of the operator, non-zero only for Crank-Nicolson.
        self._w_old = (1.0 - self.theta) * self.dt if scheme == "crank-nicolson" else 0.0

        self.ab = self._interior_matrix()

        # Response of the interior to a unit surface temperature. Constant, so
        # this is the one solve that never repeats.
        rhs_unit = numpy.zeros(nx - 1)
        rhs_unit[0] = self._a_new[1]
        self.v = scipy.linalg.solve_banded((1, 1), self.ab, rhs_unit)

        # Surface stencil, shared with the explicit path.
        self._twodz = 2.0 * (z[1] - z[0])

        self._prev = None  # BDF2 history

    # -- construction ----------------------------------------------------

    def _interior_matrix(self):
        """Banded interior system over nodes 1..nx-1, `solve_banded((1,1))`.

        The surface node is eliminated (its coupling moves to the right-hand
        side), so the unknown vector is `[T_1 ... T_{nx-1}]` and the last row
        is the adiabatic floor `T_{nx-1} - T_{nx-2} = 0`.
        """
        nx = self.nx
        m = nx - 1
        a, c = self._a_new, self._c_new

        ab = numpy.zeros((3, m))
        ab[1, :-1] = 1.0 + a[1:-1] + c[1:-1]  # nodes 1..nx-2
        ab[1, -1] = 1.0  # base row
        ab[0, 1:] = -c[1:-1]  # upper: node i couples to T[i+1]
        ab[2, :-2] = -a[2:-1]  # lower: node i couples to T[i-1]
        ab[2, -2] = -1.0  # base row: T[N] - T[N-1] = 0
        return ab

    # -- right-hand side -------------------------------------------------

    def _rhs(self, t):
        """Known part of the interior right-hand side, shape `(nx-1, n)`.

        Excludes the unknown surface temperature, which enters through `v`.
        """
        if self.scheme == "bdf2" and self._prev is not None:
            known = (4.0 * t[:, 1:] - self._prev[:, 1:]) / 3.0
        else:
            known = t[:, 1:].copy()

        if self._w_old:  # Crank-Nicolson's explicit half
            lo, mid, hi = t[:, :-2], t[:, 1:-1], t[:, 2:]
            known[:, :-1] += self._w_old * (
                self.a[1:-1] * (lo - mid) + self.c[1:-1] * (hi - mid)
            )

        known[:, -1] = 0.0  # adiabatic base row
        return numpy.ascontiguousarray(known.T)

    def _solve_interior(self, t):
        """`U` in `interior = U + T0 * v`, shape `(nx-1, n)`."""
        return scipy.linalg.solve_banded((1, 1), self.ab, self._rhs(t))

    # -- stepping --------------------------------------------------------

    def step_dirichlet(self, temperature, surface_temperature):
        """One step with the surface temperature prescribed.

        `surface_temperature` is a scalar or one value per facet.
        """
        t, squeeze = _as_2d(temperature)
        u = self._solve_interior(t)
        t0 = numpy.atleast_1d(
            numpy.asarray(surface_temperature, dtype=numpy.float64)
        )

        out = numpy.empty_like(t)
        out[:, 0] = t0
        out[:, 1:] = (u + self.v[:, None] * t0[None, :]).T
        self._advance(t)
        return out[0] if squeeze else out

    def step_radiative(self, temperature, flux, conductivity, se,
                       max_iter=100, threshold=0.1):
        """One step with the radiative surface balance, solved simultaneously.

        `flux` is the absorbed flux per facet [W/m2], `se` is
        `stefan_boltzmann * emissivity`. Returns the new profile; the surface
        and interior are converged together, so no separate boundary update
        is needed afterwards.

        `threshold` is the Newton convergence tolerance on the surface
        temperature in kelvin, matching `routine.step_surface_newton`.
        """
        t, squeeze = _as_2d(temperature)
        u = self._solve_interior(t)

        # R(T0) = flux + H - se T0^4 + G T0, with T1 = u0 + T0 v0 etc.
        g = conductivity * (-3.0 + 4.0 * self.v[0] - self.v[1]) / self._twodz
        h = conductivity * (4.0 * u[0] - u[1]) / self._twodz

        t0 = t[:, 0].copy()
        const = flux + h
        for _ in range(max_iter):
            se_t3 = se * t0 * t0 * t0
            fn = const - se_t3 * t0 + g * t0
            dfn = -4.0 * se_t3 + g
            delta = fn / dfn
            t0 -= delta
            if numpy.abs(delta).max() < threshold:
                break

        out = numpy.empty_like(t)
        out[:, 0] = t0
        out[:, 1:] = (u + self.v[:, None] * t0[None, :]).T
        self._advance(t)
        return out[0] if squeeze else out

    def _advance(self, t):
        if self.scheme == "bdf2":
            self._prev = t.copy()

    def reset(self):
        """Forget the BDF2 history, so the next step bootstraps again.

        Call after a discontinuity in the solution that the two-level history
        should not carry across -- or after restarting from a saved state.
        """
        self._prev = None


def _as_2d(temperature):
    t = numpy.asarray(temperature, dtype=numpy.float64)
    if t.ndim == 1:
        return t[None, :], True
    return t, False


# -- backwards-compatible functional interface ---------------------------


def banded_matrix(z, diffusivity, dt, scheme="backward-euler"):
    """A `Solver`, kept under the old name.

    The original returned a raw `(3, nx)` array for `scipy.solve_banded`;
    that array no longer describes the whole system, because the surface node
    is eliminated to make the non-linear boundary tractable. Returning the
    solver keeps every existing call site working -- both this and
    `step_dirichlet` below take and pass it unchanged.
    """
    return Solver(z, diffusivity, dt, scheme=scheme)


def step_dirichlet(solver, temperature, surface_temperature):
    """One implicit step with a prescribed surface temperature."""
    return solver.step_dirichlet(temperature, surface_temperature)


def step_radiative(solver, temperature, flux, conductivity, se, **kwargs):
    """One implicit step with the radiative surface balance."""
    return solver.step_radiative(temperature, flux, conductivity, se, **kwargs)
