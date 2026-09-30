#!/usr/bin/env python
"""`kalast.tpm.core.Ground` from Python: thermal properties per facet and per
layer, as numpy arrays a script changes in place.

The physics is tested in Rust (`tpm::core::tests`): the conservative stencil,
the heat flow through two materials in series, energy given back over a
rotation. This is the binding: that the arrays are the ones the steps read,
edited in place or assigned whole in any shape that broadcasts, and that
what does not fit is refused with a message that says why.

And where the columns start: `columns`, `effective_temperature` and
`mean_incidence` given one value a facet -- each latitude's own start, the
polar night's at 0 K.
"""

import numpy

from kalast.tpm import core, properties

PROP = properties.Properties(
    albedo=0.1, emissivity=0.9, density=2000.0, heat_capacity=600.0, thermal_inertia=200.0
)
PROP.compute_conductivity_diffusivity()
LAYERS, FACETS, DZ = 12, 4, 2e-3


def raises(kind, call, *words):
    try:
        call()
    except kind as e:
        for w in words:
            assert w in str(e), f"{w!r} not in {e}"
        return
    raise AssertionError(f"no {kind.__name__}")


def test_a_ground_starts_as_its_properties():
    g = core.Ground(PROP, LAYERS, FACETS)
    assert (g.layers, g.facets) == (LAYERS, FACETS)
    assert g.albedo.shape == (FACETS,) and numpy.allclose(g.albedo, 0.1)
    assert g.conductivity.shape == (LAYERS, FACETS)
    assert numpy.allclose(g.conductivity, PROP.conductivity)
    dt = core.stability_maxdt(PROP.diffusivity, DZ**2, 0.5)
    assert abs(g.stability_maxdt(DZ) - dt) <= 1e-4 * dt


def test_one_material_steps_as_its_properties_do():
    g = core.Ground(PROP, LAYERS, FACETS)
    dt = core.stability_maxdt(PROP.diffusivity, DZ**2, 0.5)
    a = core.columns(LAYERS, FACETS, 270.0)
    b = core.columns(LAYERS, FACETS, 270.0)
    cosi = numpy.array([1.0, 0.7, 0.3, 0.0])
    for _ in range(100):
        core.solar_bc(a, 1.0, cosi, PROP, DZ)
        core.heat_conduction(a, PROP, dt, DZ)
        core.solar_bc(b, 1.0, cosi, g, DZ)
        core.heat_conduction(b, g, dt, DZ)
    assert numpy.abs(a - b).max() < 1e-2, numpy.abs(a - b).max()


def test_an_array_changed_in_place_is_the_one_the_steps_read():
    g = core.Ground(PROP, LAYERS, FACETS)
    g.albedo[0] = 0.6
    g.conductivity[:3, 1] = PROP.conductivity / 20.0
    t = core.columns(LAYERS, FACETS, 270.0)
    cosi = numpy.ones(FACETS)
    for _ in range(50):
        core.solar_bc(t, 1.0, cosi, g, DZ)
        core.heat_conduction(t, g, g.stability_maxdt(DZ), DZ)
    surface = t[0]
    assert surface[0] < surface[3] - 20.0, f"the bright facet is not cooler: {surface}"
    assert surface[1] > surface[3] + 5.0, f"the insulated one is not warmer: {surface}"


def test_any_shape_that_broadcasts_is_kept_as_it_is():
    g = core.Ground(PROP, LAYERS, FACETS)
    g.density = 1500.0
    assert g.density.shape == (1, 1)
    g.heat_capacity = numpy.linspace(500.0, 800.0, FACETS)  # float64, one a facet
    assert g.heat_capacity.shape == (1, FACETS)
    g.conductivity = (PROP.conductivity * numpy.arange(1, LAYERS + 1))[:, None]
    assert g.conductivity.shape == (LAYERS, 1)
    g.emissivity = [0.9, 0.8, 0.7, 0.95]
    assert g.emissivity.dtype == core.columns(1, 1, 0.0).dtype
    t = core.columns(LAYERS, FACETS, 270.0)
    core.solar_bc(t, 1.0, numpy.ones(FACETS), g, DZ)
    core.heat_conduction(t, g, g.stability_maxdt(DZ), DZ)


def test_what_does_not_fit_is_refused_saying_why():
    g = core.Ground(PROP, LAYERS, FACETS)
    raises(ValueError, lambda: setattr(g, "albedo", numpy.ones(FACETS + 1)), "albedo", "one value a facet")
    raises(ValueError, lambda: setattr(g, "conductivity", numpy.ones(LAYERS)), "(layers, 1)", "[:, None]")
    raises(ValueError, lambda: setattr(g, "density", numpy.ones((LAYERS + 1, FACETS))), "does not broadcast")
    t = core.columns(LAYERS, FACETS, 270.0)
    raises(ValueError, lambda: core.heat_conduction(t, g, 1.01 * g.stability_maxdt(DZ), DZ), "stability_maxdt")
    raises(TypeError, lambda: core.heat_conduction(t, "rock", 1.0, DZ), "Properties", "Ground")
    other = core.columns(LAYERS + 1, FACETS, 270.0)
    raises(ValueError, lambda: core.heat_conduction(other, g, 1.0, DZ), "does not broadcast")


def test_each_column_starts_at_its_own():
    t0 = numpy.linspace(100.0, 300.0, FACETS)
    t = core.columns(LAYERS, FACETS, t0)
    assert t.shape == (LAYERS, FACETS) and t.dtype == core.columns(1, 1, 0.0).dtype
    assert numpy.allclose(t, t0[None, :])
    profile = numpy.linspace(300.0, 250.0, LAYERS)[:, None]
    assert numpy.allclose(core.columns(LAYERS, FACETS, profile)[:, 2], profile[:, 0])
    raises(ValueError, lambda: core.columns(LAYERS, FACETS, numpy.ones(FACETS + 1)), "t", "does not broadcast")


def test_a_latitude_starts_at_its_mean_sunlight():
    obliquity = numpy.radians(25.0)
    lat = numpy.radians([-90.0, -70.0, 0.0, 45.0, 90.0])
    r = core.mean_incidence(lat, obliquity)
    assert r.shape == lat.shape
    assert r[0] == 0.0 and r[1] == 0.0, f"no polar night south of -65: {r}"
    assert abs(r[2] - numpy.cos(obliquity) / numpy.pi) < 1e-6
    assert abs(r[4] - numpy.sin(obliquity)) < 1e-6, f"no polar day at the north pole: {r}"
    assert isinstance(core.mean_incidence(0.0, 0.0), float)

    t = core.effective_temperature(1.0, r, PROP.albedo, PROP.emissivity)
    assert t.shape == lat.shape and t[0] == 0.0 and t[1] == 0.0
    assert t[4] > t[2], f"at a solstice the summer pole is the warmer on average: {t}"
    one = core.effective_temperature(1.0, float(r[2]), PROP.albedo, PROP.emissivity)
    assert isinstance(one, float) and abs(t[2] - one) < 1e-3
    albedo = numpy.array([0.1, 0.1, 0.5, 0.1, 0.1])  # one a facet, as a Ground has them
    assert core.effective_temperature(1.0, r, albedo, PROP.emissivity)[2] < t[2]
    raises(ValueError, lambda: core.effective_temperature(1.0, r, numpy.ones(3), 0.9), "a", "does not broadcast")

    temperature = core.columns(LAYERS, len(lat), t)
    assert numpy.allclose(temperature[-1], t)


if __name__ == "__main__":
    # Runnable without pytest, which is not installed here.
    failures = 0
    for name, fn in sorted(globals().items()):
        if name.startswith("test_") and callable(fn):
            try:
                fn()
                print(f"ok   {name}")
            except Exception as e:
                failures += 1
                print(f"FAIL {name}\n     {type(e).__name__}: {e}")
    raise SystemExit(failures)
