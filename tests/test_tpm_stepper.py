#!/usr/bin/env python
"""`kalast.tpm.core.Stepper` from Python: a step of the columns by the scheme
of one's choice, and a ground that changes with temperature.

The schemes are tested in Rust (`tpm::stepper::tests`): the analytical wave,
their orders, a day, an eclipse, a heat flow through `k(T)`, the heat a column
gains. Here:

- the binding, and what it refuses;
- the Rust solver against the Python ones it replaces, `kalast.tpm.implicit`
  and `kalast.tpm.explicit`, which are float64 numpy: on equal layers of one
  material the two write the same equations, so they must agree to what
  float32 holds;
- a `Ground`'s two laws set from Python.
"""

import numpy

from kalast.tpm import core, explicit, implicit, properties

PROP = properties.Properties(
    albedo=0.1, emissivity=0.9, density=2000.0, heat_capacity=600.0, thermal_inertia=200.0
)
PROP.compute_conductivity_diffusivity()
SIGMA = 5.670374419e-8  # W m-2 K-4
SE = SIGMA * PROP.emissivity
LAYERS, FACETS, DZ = 24, 3, 2e-3
Z = DZ * numpy.arange(LAYERS)
LIMIT = core.stability_maxdt(PROP.diffusivity, DZ * DZ)
FLUX = numpy.array([150.0, 600.0, 1100.0])
IMPLICIT = ("backward-euler", "crank-nicolson", "bdf2")


def raises(kind, call, *words):
    try:
        call()
    except kind as e:
        for w in words:
            assert w in str(e), f"{w!r} not in {e}"
        return
    raise AssertionError(f"no {kind.__name__}")


def start():
    t = core.columns(LAYERS, FACETS, 0.0)
    t[:] = 230.0 + 15.0 * numpy.arange(FACETS) - 1.0 * numpy.arange(LAYERS)[:, None]
    return t


def python_solver(scheme, dt):
    """`implicit.Solver`, with BDF2's first step made a backward-Euler one of
    `dt`, as the Rust solver's is.

    The Python BDF2 starts by taking the step before to be the same as the
    start, which is its own matrix on the temperatures alone: a
    backward-Euler step of 2/3 `dt`, labelled `dt`. That is a first-order
    error at the first step, which a column out of balance shows as kelvins.
    """
    ref = implicit.Solver(Z, PROP.diffusivity, dt, scheme=scheme)
    if scheme != "bdf2":
        return ref
    first = implicit.Solver(Z, PROP.diffusivity, dt, scheme="backward-euler")
    state = {"started": False}

    def radiative(t, *args, **kwargs):
        if state["started"]:
            return plain_radiative(t, *args, **kwargs)
        state["started"] = True
        ref._prev = numpy.array(t, dtype=numpy.float64, copy=True)
        return first.step_radiative(t, *args, **kwargs)

    def dirichlet(t, top):
        if state["started"]:
            return plain_dirichlet(t, top)
        state["started"] = True
        ref._prev = numpy.array(t, dtype=numpy.float64, copy=True)
        return first.step_dirichlet(t, top)

    plain_radiative, plain_dirichlet = ref.step_radiative, ref.step_dirichlet
    ref.step_radiative, ref.step_dirichlet = radiative, dirichlet
    return ref


def day_flux(k, steps):
    """The flux of step `k` of `steps` a day, per facet: three latitudes."""
    sun = max(numpy.cos(2.0 * numpy.pi * k / steps), 0.0)
    return FLUX * sun


def test_a_stepper_by_name():
    for name in ("explicit", "rkc", *IMPLICIT):
        s = core.Stepper(name, stages=3)
        assert s.scheme == name and s.stages == (3 if name == "rkc" else 1)
    assert repr(core.Stepper("rkc", stages=5)) == "Stepper('rkc', stages=5)"
    assert repr(core.Stepper()) == "Stepper('explicit')"
    s = core.Stepper("bdf2", newton_threshold=1e-3, newton_iterations=7)
    assert abs(s.newton_threshold - 1e-3) < 1e-9 and s.newton_iterations == 7
    raises(ValueError, lambda: core.Stepper("leapfrog"), "no scheme named", "crank-nicolson")
    raises(ValueError, lambda: core.Stepper("rkc", stages=0), "stages")
    raises(ValueError, lambda: core.Stepper("bdf2", newton_iterations=0), "iteration")


def test_explicit_is_the_three_calls():
    dt = 0.8 * LIMIT
    ground = core.Ground(PROP, LAYERS, FACETS)
    ground.conductivity[:6] *= 0.3
    for prop, step in ((PROP, dt), (ground, 0.8 * ground.stability_maxdt(DZ))):
        a, b = start(), start()
        stepper = core.Stepper("explicit")
        for _ in range(30):
            stepper.step(a, FLUX, prop, step, DZ, bottom_flux=0.3)
            core.flux_bc(b, FLUX, prop, DZ)
            core.heat_conduction(b, prop, step, DZ)
            core.bottom_flux(b, 0.3, prop, DZ)
        assert numpy.array_equal(a, b)
        assert not numpy.array_equal(a, start())


def test_the_implicit_schemes_are_the_python_ones():
    """A day of sunlight in 200 steps, 14 times the explicit limit: the Rust
    solver in float32 against `implicit.Solver` in float64, layer by layer."""
    steps = 200
    dt = 14.0 * LIMIT
    for scheme in IMPLICIT:
        t = start()
        old = start().T.astype(numpy.float64)               # [facet][node]
        new = core.Stepper(scheme, newton_threshold=1e-4, newton_iterations=50)
        ref = python_solver(scheme, dt)
        worst = 0.0
        for k in range(steps):
            flux = day_flux(k + 1, steps)
            new.step(t, flux, PROP, dt, DZ)
            old = ref.step_radiative(old, flux, PROP.conductivity, SE, threshold=1e-7)
            worst = max(worst, numpy.abs(t.T - old).max())
        assert worst < 2e-3, f"{scheme}: {worst} K from the Python solver"
        assert t[0].max() - t[0].min() > 50.0


def test_a_surface_given_is_the_python_one():
    steps, dt = 120, 9.0 * LIMIT
    for scheme in IMPLICIT:
        t = start()
        old = start().T.astype(numpy.float64)
        new = core.Stepper(scheme)
        ref = python_solver(scheme, dt)
        for k in range(steps):
            top = 240.0 + 60.0 * numpy.sin(2.0 * numpy.pi * (k + 1) / steps + numpy.arange(FACETS))
            new.step_surface(t, top, PROP, dt, DZ)
            old = ref.step_dirichlet(old, top)
        worst = numpy.abs(t.T - old).max()
        assert worst < 2e-3, f"{scheme}: {worst} K from the Python solver"


def test_rkc_is_the_python_one():
    stages = 4
    reach = core.Stepper.rkc_stability(stages) / 2.0
    # The stability the Python module finds by scanning the polynomial.
    assert abs(core.Stepper.rkc_stability(stages) - explicit.rkc_stability_boundary(stages)) < 1e-3
    assert core.Stepper.rkc_stages(0.99 * reach) == stages
    assert core.Stepper.rkc_stages(1.01 * reach) == stages + 1
    steps, dt = 300, 0.9 * reach * LIMIT
    t = start()
    old = start().T.astype(numpy.float64)
    new = core.Stepper("rkc", stages=stages, newton_threshold=1e-4, newton_iterations=50)
    ref = explicit.Solver(Z, PROP.diffusivity, dt, scheme="rkc", stages=stages)
    worst = 0.0
    for k in range(steps):
        flux = day_flux(k, steps)
        new.step(t, flux, PROP, dt, DZ)
        old = ref.step_radiative(old, flux, PROP.conductivity, SE, threshold=1e-7)
        worst = max(worst, numpy.abs(t.T - old).max())
    assert worst < 2e-3, f"rkc: {worst} K from the Python solver"
    raises(ValueError, lambda: core.Stepper("rkc", stages=2).step(t, FLUX, PROP, dt, DZ), "unstable", "stages")


def kirchhoff(ts, q, z, kc, chi):
    """The temperature at depth `z` under a surface at `ts`, for a heat flow
    `q` through `k = kc (1 + chi (T / 350)**3)`."""
    theta = lambda t: t + chi * t**4 / (4.0 * 350.0**3)  # noqa: E731
    want = theta(ts) + q * z / kc
    lo, hi = ts, 2000.0
    for _ in range(200):
        mid = 0.5 * (lo + hi)
        lo, hi = (mid, hi) if theta(mid) < want else (lo, mid)
    return 0.5 * (lo + hi)


def test_a_ground_that_changes_with_temperature():
    kc, chi, q, layers = 0.005, 2.7, 12.0, 41
    dz = 0.2 / (layers - 1)
    ground = core.Ground(PROP, layers, 2)
    assert ground.radiative_conductivity == 0.0 and ground.heat_capacity_polynomial is None
    plain = ground.stability_maxdt(dz)
    ground.conductivity[:] = kc
    constant = ground.stability_maxdt(dz)
    ground.radiative_conductivity = chi
    assert abs(ground.radiative_conductivity - chi) < 1e-6
    # The conductivity at 450 K is 1 + 2.7 (450 / 350)**3 = 6.74 times kc.
    hot = ground.stability_maxdt(dz, temperatures=(90.0, 450.0))
    assert abs(constant / hot - (1.0 + chi * (450.0 / 350.0) ** 3)) < 1e-3, constant / hot
    assert constant > plain

    tops = numpy.array([100.0, 250.0])
    t = core.columns(layers, 2, 200.0)
    stepper = core.Stepper("backward-euler")
    for _ in range(400):
        stepper.step_surface(t, tops, ground, 1e7, dz, bottom_flux=q)
    want = numpy.array([[kirchhoff(ts, q, i * dz, kc, chi) for ts in tops] for i in range(layers)])
    worst = numpy.abs(t[1:] - want[1:]).max()
    assert worst < 0.03, f"{worst} K off the closed form"
    assert t[-1, 1] > 380.0

    # The explicit step holds what the implicit one found.
    held = t.copy()
    explicit_step = core.Stepper("explicit")
    for _ in range(20):
        explicit_step.step_surface(held, tops, ground, 0.8 * hot, dz, bottom_flux=q)
    assert numpy.abs(held - t).max() < 5e-3


def test_a_heat_capacity_that_grows_with_temperature():
    moon = [-3.6125, 2.7431, 2.3616e-3, -1.2340e-5, 8.9093e-9]
    layers, dz, q = 30, 2e-3, 60.0
    quiet = properties.Properties(
        albedo=0.1, emissivity=0.0, density=2000.0, heat_capacity=600.0, thermal_inertia=200.0
    )
    quiet.compute_conductivity_diffusivity()
    ground = core.Ground(quiet, layers, 1)
    ground.heat_capacity_polynomial = moon
    assert numpy.allclose(ground.heat_capacity_polynomial, moon, rtol=1e-6)
    ground.heat_capacity_polynomial = moon[:2]                  # fewer: the rest are 0
    assert ground.heat_capacity_polynomial[2:] == [0.0, 0.0, 0.0]
    ground.heat_capacity_polynomial = moon
    limit = ground.stability_maxdt(dz, temperatures=(150.0, 450.0))

    def enthalpy(temp):
        temp = numpy.asarray(temp, numpy.float64)
        return sum(a * temp ** (n + 1) / (n + 1) for n, a in enumerate(moon))

    for scheme, dt in (("explicit", 0.8 * limit), ("bdf2", 6.0 * limit)):
        steps = round(100_000.0 / dt)
        t = core.columns(layers, 1, 200.0)
        stepper = core.Stepper(scheme)
        for _ in range(steps):
            stepper.step(t, [q], ground, dt, dz)
        gained = 0.5 * (enthalpy(t[0, 0]) - enthalpy(200.0)) + (enthalpy(t[1:-1, 0]) - enthalpy(200.0)).sum()
        gained *= quiet.density * dz
        absorbed = q * dt * steps
        assert abs(gained / absorbed - 1.0) < 0.01, f"{scheme}: gained {gained} J/m2 of {absorbed}"
        assert t[0, 0] > 285.0

    ground.heat_capacity_polynomial = None
    assert ground.heat_capacity_polynomial is None
    raises(ValueError, lambda: setattr(ground, "heat_capacity_polynomial", [1.0] * 6), "1 to 5")
    raises(ValueError, lambda: setattr(ground, "radiative_conductivity", -1.0), "not negative")
    ground.heat_capacity_polynomial = [-50.0, 0.1]              # negative under 500 K
    raises(ValueError, lambda: ground.stability_maxdt(dz, temperatures=(150.0, 450.0)), "not positive")
    raises(ValueError, lambda: ground.stability_maxdt(dz, temperatures=(450.0, 150.0)), "the first the lower")


def test_what_cannot_be_stepped_is_refused():
    for scheme in ("explicit", "rkc", "bdf2"):
        stepper = core.Stepper(scheme, stages=2)
        t = start()
        raises(ValueError, lambda: stepper.step(t, FLUX[:2], PROP, 50.0, DZ), "2 absorbed fluxes for 3 facets")
        raises(ValueError, lambda: stepper.step(t, [1.0, -5.0, 2.0], PROP, 50.0, DZ), "facet 1")
        raises(ValueError, lambda: stepper.step_surface(t, [250.0, 0.0, 250.0], PROP, 50.0, DZ), "facet 1")
        raises(ValueError, lambda: stepper.step(t, FLUX, PROP, 0.0, DZ), "positive")
        raises(ValueError, lambda: stepper.step(t, FLUX, PROP, 50.0, DZ, bottom_flux=float("inf")), "finite")
        raises(TypeError, lambda: stepper.step(t, FLUX, "rock", 50.0, DZ), "Properties")
        raises(TypeError, lambda: stepper.step(t.astype(numpy.float64), FLUX, PROP, 50.0, DZ), "float32")
        assert numpy.array_equal(t, start()), scheme


if __name__ == "__main__":
    for name, fn in list(globals().items()):
        if name.startswith("test_") and callable(fn):
            fn()
            print("ok  ", name)
