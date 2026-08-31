"""Explicit conduction: forward Euler, and super-time-stepping.

The implicit schemes in `kalast.tpm.implicit` buy an unbounded timestep at
the price of a banded solve. This module asks the other question: keeping the
scheme explicit -- no linear algebra, no matrix, trivially vectorised over
facets -- how much larger can the timestep be made?

What does *not* help
--------------------

The instinct from ODE work is to reach for a higher-order explicit method.
For diffusion that is a poor trade. The eigenvalues of the discrete Laplacian
are real and negative, spread over `[-4D/h^2, 0]`, and what limits the
timestep is how far down the negative real axis a scheme's stability region
extends -- not its order. Forward Euler reaches `|z| = 2`; classical RK4
reaches 2.79, for four times the work per step. That is a net loss.

Nor does raising the *spatial* order help: a fourth-order stencil resolves
the wave better per node but widens the eigenvalue spread, tightening the
same limit.

DuFort-Frankel is the other classical answer -- an explicit three-level
scheme that is unconditionally stable. It is not offered here because its
stability is bought with consistency rather than accuracy: its truncation
error carries a term in `(dt/h)^2`, so it converges to the heat equation only
if `dt/h -> 0`. On a geometric grid whose first layer is a millimetre, a
timestep large enough to be worth having makes that term large, and the
scheme quietly solves a different equation. Unconditional stability with no
accuracy warning is exactly the kind of trap this module exists to avoid.

What does help
--------------

Super-time-stepping. Instead of one forward-Euler step, take `s` substeps
with *unequal*, deliberately chosen lengths -- some individually unstable --
whose composite amplification polynomial is a shifted Chebyshev polynomial.
Chebyshev polynomials are the ones that stay within `[-1, 1]` over the
longest possible interval, so the composite stays stable over a stretch of
the negative real axis growing like `s^2`.

`s` substeps therefore advance `~s^2` times as far as one forward-Euler step,
at `s` times the cost: a speedup of order `s`. Ten stages is roughly a
six-fold saving here, and unlike the implicit path it needs no solve, so it
vectorises across facets exactly as the current code does.

The scheme implemented is first-order RKC (Runge-Kutta-Chebyshev) in Verwer's
damped form. The damping `eps` pulls the polynomial slightly inside the unit
circle so the stability region has width around the real axis rather than
touching it at isolated points -- without it, any eigenvalue landing between
two touch points is unstable, and a variable-spacing grid guarantees some
will.

Accuracy is first order in `dt`, like forward Euler; the gain is stability,
not order. That is the catch, and it decides the recommendation.

Where it lands here
-------------------

Measured on the analytical wave (`examples/analytical/sinusoidal.py`), 4
spins of Didymos, 16-node geometric grid, error against a time-converged
reference on the same grid:

    scheme                    dt [s]   steps   stages   err [K]
    forward Euler                9.0    3636        1     0.588
    RKC                         89.7     363        3     4.489
    implicit BDF2               81.4     400        -     0.023

RKC is three times faster than forward Euler and stays stable out to 200x its
limit -- the stability claim holds. But it is beaten outright by the implicit
path, which at the same timestep is both faster and two orders of magnitude
more accurate, because a tridiagonal solve batched across facets costs less
than three explicit stages.

So for the 1D column this module is the *second* choice, and it is here for
the case where it becomes the first: any problem where the solve stops being
cheap. Lateral conduction or an FEM discretisation gives a matrix that is no
longer tridiagonal, and a GPU implementation has no banded solver at all --
in both, RKC keeps its `s^2` timestep with nothing but array arithmetic.
Those are exactly the directions the roughness and lateral-heating work in
`notes/2026-08-27_conduction_solvers/` points at, which is why this was
worth measuring rather than assuming either way.
"""

import functools

import numpy

SCHEMES = ("forward-euler", "rkc")

#: Verwer's standard damping for RKC. Larger values widen the stability
#: region around the real axis and shorten it.
DEFAULT_DAMPING = 2.0 / 13.0


def _chebyshev(w0, s):
    """`T_j(w0)` for `j = 0..s`, by the three-term recursion."""
    t = numpy.empty(s + 1)
    t[0] = 1.0
    if s >= 1:
        t[1] = w0
    for j in range(2, s + 1):
        t[j] = 2.0 * w0 * t[j - 1] - t[j - 2]
    return t


def _chebyshev_derivative(w0, s):
    """`T'_j(w0)` for `j = 0..s`, by the differentiated recursion."""
    t = _chebyshev(w0, s)
    d = numpy.zeros(s + 1)
    if s >= 1:
        d[1] = 1.0
    for j in range(2, s + 1):
        d[j] = 2.0 * t[j - 1] + 2.0 * w0 * d[j - 1] - d[j - 2]
    return d


def rkc_coefficients(s, damping=DEFAULT_DAMPING):
    """Recursion coefficients for `s`-stage first-order damped RKC.

    Returns `(mu_tilde_1, mu, nu, mu_tilde)`, where the stage recursion is

        Y1 = Y0 + mu_tilde_1 dt F(Y0)
        Yj = mu[j] Y_{j-1} + nu[j] Y_{j-2} + mu_tilde[j] dt F(Y_{j-1})

    and `mu[j] + nu[j] = 1`, which is what makes the method first-order
    consistent regardless of `s`.
    """
    if s < 1:
        raise ValueError("s must be at least 1")
    w0 = 1.0 + damping / (s * s)
    t = _chebyshev(w0, s)
    d = _chebyshev_derivative(w0, s)
    w1 = t[s] / d[s]

    mu = numpy.zeros(s + 1)
    nu = numpy.zeros(s + 1)
    mu_tilde = numpy.zeros(s + 1)
    for j in range(2, s + 1):
        mu[j] = 2.0 * w0 * t[j - 1] / t[j]
        nu[j] = -t[j - 2] / t[j]
        mu_tilde[j] = 2.0 * w1 * t[j - 1] / t[j]
    return w1 / w0, mu, nu, mu_tilde


@functools.lru_cache(maxsize=None)
def rkc_stability_boundary(s, damping=DEFAULT_DAMPING, tol=1e-6):
    """How far down the negative real axis `s` stages stay stable.

    Returns `beta` such that the method is stable for `|lambda| dt <= beta`.
    Forward Euler is `beta = 2`, so `beta / 2` is the factor by which the
    timestep may be raised, and `beta / (2 s)` the net saving after paying
    for `s` stages.

    Computed by scanning the amplification polynomial rather than quoted from
    a table, so it stays correct if the damping is changed. The scan is
    coarse and then bisected, because the polynomial is cheap in bulk but the
    interval to search grows like `s^2`.
    """
    mu_tilde_1, mu, nu, mu_tilde = rkc_coefficients(s, damping)

    def amplification(z):
        """`R_s(z)`, evaluated over a whole array of `z` at once."""
        z = numpy.asarray(z, dtype=numpy.float64)
        y_prev2 = numpy.ones_like(z)
        y_prev = 1.0 + mu_tilde_1 * z
        for j in range(2, s + 1):
            y = mu[j] * y_prev + nu[j] * y_prev2 + mu_tilde[j] * z * y_prev
            y_prev2, y_prev = y_prev, y
        return y_prev

    # Stability grows like s^2; scan past that, and take the *first* crossing
    # so an isolated stable island beyond it is not mistaken for the boundary.
    hi = 2.5 * s * s + 4.0
    zs = numpy.linspace(0.0, hi, max(20000, 400 * s))
    unstable = numpy.abs(amplification(-zs)) > 1.0
    unstable[0] = False  # R(0) = 1 exactly; rounding must not read as unstable
    idx = int(numpy.argmax(unstable))
    if not unstable[idx]:
        # Never silently return the end of the scan: an over-estimated
        # boundary is an unstable run, which is worse than a failure here.
        raise RuntimeError(
            f"no stability boundary found for s={s} below {hi:.4g}; the "
            "Chebyshev recursion has probably lost precision at this many "
            "stages -- use fewer stages, or the implicit solver"
        )

    lo_z, hi_z = zs[idx - 1], zs[idx]
    while hi_z - lo_z > tol:
        mid = 0.5 * (lo_z + hi_z)
        if abs(amplification(-mid)) > 1.0:
            hi_z = mid
        else:
            lo_z = mid
    return float(lo_z)


def stages_for(speedup, damping=DEFAULT_DAMPING):
    """Smallest `s` whose timestep is at least `speedup` x forward Euler's."""
    s = 1
    while rkc_stability_boundary(s, damping) / 2.0 < speedup:
        s += 1
        if s > 200:
            raise ValueError("speedup too large to reach with RKC")
    return s


class Solver:
    """Explicit conduction for a column, or every facet's column at once.

    Mirrors `kalast.tpm.implicit.Solver`, so the two are interchangeable at
    the call site and a run can be switched between them by name.

    Parameters
    ----------
    z, diffusivity, dt
        As for the implicit solver.
    scheme : str
        One of `SCHEMES`.
    stages : int, optional
        RKC stages. If omitted, the smallest `s` that makes `dt` stable is
        chosen automatically -- which is the sensible default, because taking
        more stages than needed costs work without buying anything.
    """

    def __init__(self, z, diffusivity, dt, scheme="forward-euler",
                 stages=None, damping=DEFAULT_DAMPING):
        if scheme not in SCHEMES:
            raise ValueError(f"scheme must be one of {SCHEMES}, got {scheme!r}")

        z = numpy.asarray(z, dtype=numpy.float64)
        if z.ndim != 1 or z.size < 4:
            raise ValueError("z must be 1-D with at least 4 nodes")

        self.z = z
        self.dt = float(dt)
        self.scheme = scheme
        self.nx = nx = z.size
        self.damping = damping

        dz = numpy.diff(z)
        d = numpy.broadcast_to(
            numpy.asarray(diffusivity, dtype=numpy.float64), (nx,)
        )
        h_lo, h_hi = dz[:-1], dz[1:]
        total = h_lo + h_hi
        # D times the variable-spacing second difference weights, no dt: the
        # stages each apply their own multiple of dt.
        self.a = d[1:-1] * 2.0 / (h_lo * total)
        self.c = d[1:-1] * 2.0 / (h_hi * total)

        # Largest eigenvalue magnitude of the discrete operator, bounded by
        # Gershgorin: the tightest node sets the whole column's limit.
        self.spectral_radius = float(2.0 * (self.a + self.c).max())
        self.max_dt_forward_euler = 2.0 / self.spectral_radius

        if scheme == "rkc":
            need = self.dt / self.max_dt_forward_euler
            self.stages = stages if stages is not None else stages_for(need, damping)
            self.beta = rkc_stability_boundary(self.stages, damping)
            if self.dt > self.beta / self.spectral_radius:
                raise ValueError(
                    f"dt={self.dt:.3g}s needs more than {self.stages} RKC stages "
                    f"(stable to {self.beta / self.spectral_radius:.3g}s); "
                    "raise stages or lower dt"
                )
            self._coefs = rkc_coefficients(self.stages, damping)
            self.speedup = self.beta / 2.0 / self.stages
        else:
            self.stages = 1
            self.beta = 2.0
            self.speedup = 1.0
            if self.dt > self.max_dt_forward_euler:
                raise ValueError(
                    f"dt={self.dt:.3g}s exceeds the forward-Euler stability "
                    f"limit {self.max_dt_forward_euler:.3g}s; use scheme='rkc' "
                    "or kalast.tpm.implicit"
                )

        self._twodz = 2.0 * (z[1] - z[0])

    # -- operator --------------------------------------------------------

    def _laplacian(self, t):
        """`D d2T/dz2` at the interior nodes, shape `(n, nx-2)`."""
        lo, mid, hi = t[:, :-2], t[:, 1:-1], t[:, 2:]
        return self.a * (lo - mid) + self.c * (hi - mid)

    def _apply_boundaries(self, t, surface, flux, conductivity, se, threshold):
        """Set the surface and base nodes of `t` in place."""
        if surface is not None:
            t[:, 0] = surface
        else:
            _surface_newton(t, flux, se, conductivity, self._twodz,
                            threshold=threshold)
        t[:, -1] = t[:, -2]

    def _step(self, temperature, surface=None, flux=None, conductivity=None,
              se=None, threshold=0.1):
        t, squeeze = _as_2d(temperature)
        out = t.copy()
        self._apply_boundaries(out, surface, flux, conductivity, se, threshold)

        if self.scheme == "forward-euler":
            out[:, 1:-1] += self.dt * self._laplacian(out)
            self._apply_boundaries(out, surface, flux, conductivity, se, threshold)
            return out[0] if squeeze else out

        # RKC: the boundary is re-imposed at every stage, because the surface
        # node is algebraic -- quasi-steady against the flux balance -- rather
        # than something the stages integrate.
        mu_tilde_1, mu, nu, mu_tilde = self._coefs
        y0 = out
        y_prev2 = y0
        y_prev = y0.copy()
        y_prev[:, 1:-1] += mu_tilde_1 * self.dt * self._laplacian(y0)
        self._apply_boundaries(y_prev, surface, flux, conductivity, se, threshold)

        for j in range(2, self.stages + 1):
            y = mu[j] * y_prev + nu[j] * y_prev2
            y[:, 1:-1] += mu_tilde[j] * self.dt * self._laplacian(y_prev)
            self._apply_boundaries(y, surface, flux, conductivity, se, threshold)
            y_prev2, y_prev = y_prev, y

        return y_prev[0] if squeeze else y_prev

    # -- stepping --------------------------------------------------------

    def step_dirichlet(self, temperature, surface_temperature):
        """One step with the surface temperature prescribed."""
        return self._step(temperature, surface=surface_temperature)

    def step_radiative(self, temperature, flux, conductivity, se,
                       threshold=0.1):
        """One step with the radiative surface balance."""
        return self._step(temperature, flux=flux, conductivity=conductivity,
                          se=se, threshold=threshold)

    def reset(self):
        """No history to discard; present so the two solvers match."""


def _surface_newton(t, flux, se, conductivity, twodz, max_iter=100,
                    threshold=0.1):
    """`routine.step_surface_newton` on a `(n, nx)` array, in place."""
    t0 = t[:, 0].copy()
    t1, t2 = t[:, 1], t[:, 2]
    for _ in range(max_iter):
        se_t3 = se * t0 * t0 * t0
        fn = flux - se_t3 * t0 + conductivity * (-3.0 * t0 + 4.0 * t1 - t2) / twodz
        dfn = -4.0 * se_t3 - 3.0 * conductivity / twodz
        delta = fn / dfn
        t0 -= delta
        if numpy.abs(delta).max() < threshold:
            break
    t[:, 0] = t0


def _as_2d(temperature):
    t = numpy.asarray(temperature, dtype=numpy.float64)
    if t.ndim == 1:
        return t[None, :], True
    return t, False
