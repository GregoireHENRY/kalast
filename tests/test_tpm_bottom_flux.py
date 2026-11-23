#!/usr/bin/env python
"""A heat flow through the base of the columns, on the CPU and on the GPU:
`kalast.tpm.core.bottom_flux` and `GpuTpm.set_bottom_flux`.

The CPU's function is tested in Rust (`tpm::core::tests::bottom_flux_*`). Here:
its binding, and that the GPU holds the same steady state -- a column carrying
a heat flow `q` to a surface that radiates it, `T = T0 + q z / k`, which a step
must leave where it is.
"""

import numpy

from kalast.tpm import core, properties

PROP = properties.Properties(
    albedo=0.1, emissivity=0.9, density=2000.0, heat_capacity=600.0, thermal_inertia=200.0
)
PROP.compute_conductivity_diffusivity()
LAYERS, FACETS, DZ = 12, 4, 2e-3
SIGMA = 5.670374419e-8  # W m-2 K-4
# Large beside the Moon's 0.018 W/m2, so that float32 resolves the gradient:
# half a kelvin a layer.
Q = 0.5 * PROP.conductivity / DZ


def raises(kind, call, *words):
    try:
        call()
    except kind as e:
        for w in words:
            assert w in str(e), f"{w!r} not in {e}"
        return
    raise AssertionError(f"no {kind.__name__}")


def steady():
    t = core.columns(LAYERS, FACETS, 0.0)
    t[:] = 250.0 + 5.0 * numpy.arange(FACETS) + (Q * DZ / PROP.conductivity) * numpy.arange(LAYERS)[:, None]
    return t


def test_no_heat_flow_is_the_adiabatic_bottom():
    a, b = steady(), steady()
    core.bottom_adiabatic(a)
    core.bottom_flux(b, 0.0, PROP, DZ)
    assert numpy.array_equal(a, b)


def test_the_cpu_holds_the_steady_state():
    t = steady()
    dt = 0.2 * DZ**2 / PROP.diffusivity
    for _ in range(200):
        core.heat_conduction(t, PROP, dt, DZ)
        core.bottom_flux(t, Q, PROP, DZ)
    assert numpy.abs(t - steady()).max() < 1e-9
    assert numpy.allclose(t[-1] - t[-2], Q * DZ / PROP.conductivity, rtol=0, atol=1e-10)


def test_what_cannot_be_is_refused():
    t = steady()
    raises(ValueError, lambda: core.bottom_flux(t, float("nan"), PROP, DZ), "finite")
    assert numpy.array_equal(t, steady())


def test_the_gpu_holds_the_same_steady_state():
    from kalast._rs.tpm.gpu import GpuTpm

    dt = 0.2 * DZ**2 / PROP.diffusivity
    coef = numpy.full(LAYERS - 2, dt / DZ**2, numpy.float32)
    se = PROP.emissivity * SIGMA
    try:
        gpu = GpuTpm(FACETS, coef, coef, numpy.full(LAYERS, PROP.diffusivity, numpy.float32),
                     numpy.float32(se), numpy.float32(PROP.conductivity), numpy.float32(2 * DZ),
                     numpy.float32(1e-4), 100)
    except RuntimeError as e:
        print(f"     (no GPU here: {e})")
        return
    start = numpy.ascontiguousarray(steady().T, numpy.float32)   # [facet][node]
    # The surface radiates what it absorbs and what the column brings it.
    flux = (se * start[:, 0].astype(numpy.float64) ** 4 - Q).astype(numpy.float32)

    gpu.upload(start)
    gpu.set_bottom_flux(Q, DZ)
    for _ in range(200):
        gpu.step(flux)
    held = gpu.download()
    assert numpy.abs(held - start).max() < 2e-3, numpy.abs(held - start).max()
    assert numpy.allclose(held[:, -1] - held[:, -2], Q * DZ / PROP.conductivity, rtol=0, atol=1e-3)

    # Without it the base is adiabatic, and the same column cools from below.
    gpu.upload(start)
    gpu.set_bottom_flux(0.0, DZ)
    for _ in range(200):
        gpu.step(flux)
    loose = gpu.download()
    assert numpy.all(loose[:, -1] == loose[:, -2])
    assert numpy.abs(loose - start).max() > 0.1

    raises(ValueError, lambda: gpu.set_bottom_flux(float("nan"), DZ), "finite")
    raises(ValueError, lambda: gpu.set_bottom_flux(Q, 0.0), "spacing")


if __name__ == "__main__":
    for name, fn in list(globals().items()):
        if name.startswith("test_") and callable(fn):
            fn()
            print("ok  ", name)
