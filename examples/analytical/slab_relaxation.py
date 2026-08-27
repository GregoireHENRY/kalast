#!/usr/bin/env python
"""Validate conduction against Fourier-series solutions for a finite slab.

Complements `sinusoidal.py`. That one drives the surface periodically and
checks the steady damped wave; this one starts from an initial profile and
watches it relax, which exercises the *transient* response and both kinds of
boundary the thermophysical model uses:

- **Dirichlet**, temperature pinned at both faces. With `T=0` at `z=0` and
  `z=L`, the solution is a sine series

      T(z,t) = sum_n  Dn sin(n pi z / L) exp(-D (n pi / L)^2 t)
      Dn     = (2/L) integral T0(z) sin(n pi z / L) dz

- **Neumann**, zero flux at both faces — the adiabatic floor a TPM column
  uses at depth. The series is a cosine one and conserves total heat, so it
  relaxes to the mean of the initial profile rather than to zero:

      T(z,t) = B0 + sum_n Bn cos(n pi z / L) exp(-D (n pi / L)^2 t)
      B0     = (1/L) integral T0 dz,   Bn = (2/L) integral T0 cos(...) dz

Replaces `examples/old/analytical/{setup,dirichlet,neumann}.py`, which split
one calculation across three modules that had to be imported by relative
name, and called `kalast.tpm.core.diffusivity` -- since moved to
`kalast.tpm.properties`.
"""

import numpy
import scipy.integrate
from matplotlib import pyplot

import kalast
import kalast.tpm.properties as properties
import kalast.tpm.routine as routine

# Same slab as the original example: a lab-scale sample, not an asteroid, so
# the relaxation completes in hours and the series converges quickly.
RHO = 2500.0
HEAT_CAPACITY = 600.0
K = 0.081667
D = properties.diffusivity(K, RHO, HEAT_CAPACITY)

L = 0.1  # slab thickness [m]
N_MODES = 100
NZ = 100
SNAPSHOTS = numpy.array([5 * 60.0, 3600.0, 4 * 3600.0, 10 * 3600.0, 40 * 3600.0])

z = numpy.linspace(0.0, L, NZ)


def dirichlet_initial():
    """Uniform 300 K interior with both faces held at 0 K."""
    T = numpy.full(NZ, 300.0)
    T[0] = 0.0
    T[-1] = 0.0
    return T


def neumann_initial():
    """Exponential profile, insulated at both faces."""
    return 300.0 * numpy.exp(-z * 1.0)


def dirichlet_analytical(T0, t):
    coef = numpy.array([
        2.0 / L * scipy.integrate.simpson(T0 * numpy.sin((n + 1) * numpy.pi * z / L), x=z)
        for n in range(N_MODES)
    ])
    out = numpy.zeros_like(z)
    for n in range(N_MODES):
        kn = (n + 1) * numpy.pi / L
        out += coef[n] * numpy.sin(kn * z) * numpy.exp(-D * kn * kn * t)
    return out


def neumann_analytical(T0, t):
    b0 = scipy.integrate.simpson(T0, x=z) / L
    out = numpy.full_like(z, b0)
    for n in range(1, N_MODES):
        kn = n * numpy.pi / L
        bn = 2.0 / L * scipy.integrate.simpson(T0 * numpy.cos(kn * z), x=z)
        out += bn * numpy.cos(kn * z) * numpy.exp(-D * kn * kn * t)
    return out


def run(T0, boundary, dt):
    """March the explicit solver, snapshotting at SNAPSHOTS."""
    coefs = routine.uniform_coefficients(z, dt)
    d = numpy.full(NZ, D, dtype=numpy.float32)
    T = T0.astype(numpy.float32).copy()

    snaps = numpy.zeros((SNAPSHOTS.size, NZ))
    taken = 0
    t = 0.0
    while taken < SNAPSHOTS.size:
        T[1:-1] = kalast.tpm.core.conduction_1d(T, d, coefs)
        if boundary == "dirichlet":
            T[0] = 0.0
            T[-1] = 0.0
        else:  # zero flux at both faces
            T[0] = T[1]
            T[-1] = T[-2]
        t += dt
        if t >= SNAPSHOTS[taken]:
            snaps[taken] = T
            taken += 1
    return snaps


dz = z[1] - z[0]
dt = 0.4 * kalast.tpm.core.stability_maxdt(D, dz * dz)
print(f"slab L={L} m, D={D:.4e} m2/s, {NZ} nodes, dz={dz * 1000:.2f} mm, dt={dt:.2f} s")
print(f"diffusion time L^2/D = {L * L / D / 3600:.1f} h, "
      f"integrating to {SNAPSHOTS[-1] / 3600:.0f} h\n")

kalast.plot.style.load()
fig, axes = pyplot.subplots(1, 2, figsize=(11.5, 4.8), sharey=True)

for ax, name, initial, exact in [
    (axes[0], "dirichlet", dirichlet_initial, dirichlet_analytical),
    (axes[1], "neumann", neumann_initial, neumann_analytical),
]:
    T0 = initial()
    snaps = run(T0, name, dt)

    errs = []
    for i, t in enumerate(SNAPSHOTS):
        ref = exact(T0, t)
        errs.append(numpy.abs(snaps[i] - ref).max())
        (l1,) = ax.plot(snaps[i], z * 100, lw=1.4, color="k")
        (l2,) = ax.plot(ref, z * 100, lw=1.2, ls="--", color="r")
    l1.set_label("numerical")
    l2.set_label("analytical (Fourier)")

    print(f"{name:10s} max|err| per snapshot [K]: "
          + "  ".join(f"{e:6.3f}" for e in errs))

    ax.set_xlabel("temperature [K]")
    ax.set_title(f"{name} — max error {max(errs):.2f} K", fontsize=10)
    ax.set_ylim(L * 100, 0)
    ax.grid(alpha=0.3)

axes[0].set_ylabel("depth [cm]")
axes[0].legend(fontsize=9)
fig.suptitle(
    "Slab relaxation: conduction solver vs Fourier-series solutions "
    f"(D={D:.3e} m2/s, snapshots at {', '.join(f'{t / 3600:g}h' for t in SNAPSHOTS)})",
    fontsize=11,
)
fig.tight_layout()
fig.savefig("out/analytical_slab_relaxation.png", dpi=150)
print("\nwrote out/analytical_slab_relaxation.png")
