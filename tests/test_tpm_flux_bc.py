#!/usr/bin/env python
"""`kalast.tpm.core.flux_bc` from Python: the surface balance from the flux each
facet absorbs -- the direct sunlight, and whatever else a surface absorbs.

The balance is tested in Rust (`tpm::core::tests::flux_bc_*`). This is the
binding: the result of `solar_bc` for the sunlight alone, a warmer surface for
more flux, the columns changed in place, and what cannot be a flux refused
with a message that says why.
"""

import numpy

from kalast.tpm import core, properties

PROP = properties.Properties(
    albedo=0.1, emissivity=0.9, density=2000.0, heat_capacity=600.0, thermal_inertia=200.0
)
PROP.compute_conductivity_diffusivity()
LAYERS, FACETS, DZ, DAU = 12, 4, 2e-3, 1.3
COSI = numpy.array([1.0, 0.7, 0.0, -0.4])
SIGMA = 5.670374419e-8  # W m-2 K-4


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
    t[:] = 220.0 + 15.0 * numpy.arange(FACETS) - 1.5 * numpy.arange(LAYERS)[:, None]
    return t


def test_the_sunlight_alone_is_solar_bc():
    flux = numpy.array([core.radiation_sun(DAU, c, PROP.albedo) for c in COSI])
    a, b = start(), start()
    core.solar_bc(a, DAU, COSI, PROP, DZ)
    core.flux_bc(b, flux, PROP, DZ)
    assert numpy.array_equal(a, b), numpy.abs(a - b).max()
    assert not numpy.array_equal(a, start()), "the surfaces moved"


def test_a_ground_gives_each_facet_its_emissivity():
    g = core.Ground(PROP, LAYERS, FACETS)
    g.albedo[:] = [0.05, 0.1, 0.2, 0.4]
    g.emissivity[:] = [0.8, 0.9, 0.95, 1.0]
    flux = numpy.array([core.radiation_sun(DAU, c, al) for c, al in zip(COSI, g.albedo)])
    a, b = start(), start()
    core.solar_bc(a, DAU, COSI, g, DZ)
    core.flux_bc(b, flux, g, DZ)
    assert numpy.array_equal(a, b), numpy.abs(a - b).max()


def test_more_flux_is_a_warmer_surface_and_only_the_surface_moves():
    sun = numpy.array([core.radiation_sun(DAU, c, PROP.albedo) for c in COSI])
    alone, heated = start(), start()
    core.flux_bc(alone, sun, PROP, DZ)
    core.flux_bc(heated, sun + 40.0, PROP, DZ)  # 40 W/m2 from a companion, say
    rise = heated[0] - alone[0]
    assert (rise > 0).all(), rise
    # dT = dQ / (4 e sigma T^3 + 3 k / (2 dz)), each solve being good to Newton's 0.1 K
    slope = 4 * PROP.emissivity * SIGMA * alone[0] ** 3 + 3 * PROP.conductivity / (2 * DZ)
    assert numpy.abs(rise - 40.0 / slope).max() < 0.2, (rise, 40.0 / slope)
    assert numpy.array_equal(heated[1:], start()[1:])


def test_what_cannot_be_a_flux_is_refused():
    t = start()
    raises(ValueError, lambda: core.flux_bc(t, numpy.ones(FACETS - 1), PROP, DZ), "absorbed fluxes", "facets")
    raises(ValueError, lambda: core.flux_bc(t, numpy.array([1.0, -1.0, 0.0, 0.0]), PROP, DZ), "facet 1", "negative")
    raises(ValueError, lambda: core.flux_bc(t, numpy.array([1.0, 1.0, numpy.nan, 0.0]), PROP, DZ), "facet 2")
    assert numpy.array_equal(t, start()), "an error changes nothing"


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
