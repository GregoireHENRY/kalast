#!/usr/bin/env python
"""`GpuStepper` from Python: the thermal columns stepped on the GPU by any
scheme, against `kalast.tpm.core.Stepper` on the CPU.

The two are tested side by side in Rust (`tpm::gpu_stepper::tests`), scheme by
scheme. Here: the binding -- the same columns, the same `Properties` or
`Ground`, the same calls on both -- on the kind of ground the lunar case needs:
graded, denser with depth, with the Moon's laws and its heat flow.

Without a GPU the tests say so and check nothing.
"""

import numpy

from kalast.tpm import core, properties

PROP = properties.Properties(
    albedo=0.1, emissivity=0.9, density=2000.0, heat_capacity=600.0, thermal_inertia=120.0
)
PROP.compute_conductivity_diffusivity()
FACETS, DZ = 4, 1.5e-3
PEAK = numpy.array([100.0, 400.0, 800.0, 1200.0], numpy.float32)
MOON = [-3.6125, 2.7431, 2.3616e-3, -1.2340e-5, 8.9093e-9]
SCHEMES = ("explicit", "rkc", "backward-euler", "crank-nicolson", "bdf2")


def raises(kind, call, *words):
    try:
        call()
    except kind as e:
        for w in words:
            assert w in str(e), f"{w!r} not in {e}"
        return
    raise AssertionError(f"no {kind.__name__}")


def gpu_class():
    """`GpuStepper`, or `None` where no GPU answers."""
    from kalast._rs.tpm.gpu import GpuContext, GpuStepper

    try:
        return GpuStepper, GpuContext()
    except RuntimeError as e:
        print(f"     (no GPU here, nothing checked: {e})")
        return None, None


def lunar_ground():
    ground = core.Ground.graded(PROP, FACETS, DZ, 0.4, ratio=1.25)
    layers = ground.layers
    depth = numpy.arange(layers, dtype=numpy.float32)[:, None]
    # Denser with depth, and a little different under each facet.
    ground.density = ground.density * (1.0 + 0.02 * depth) * (1.0 - 0.05 * numpy.arange(FACETS, dtype=numpy.float32))
    ground.emissivity = [0.95, 0.9, 0.85, 0.8]
    ground.radiative_conductivity = 2.7
    ground.heat_capacity_polynomial = MOON
    return ground


def start(layers):
    t = core.columns(layers, FACETS, 0.0)
    t[:] = 230.0 + 9.0 * numpy.arange(FACETS) - 0.8 * numpy.arange(layers)[:, None]
    return t


def step_of(scheme, limit):
    return {"explicit": 0.8, "rkc": 3.0}.get(scheme, 12.0) * limit


def test_every_scheme_on_the_gpu_is_the_cpus():
    GpuStepper, ctx = gpu_class()
    if GpuStepper is None:
        return
    ground = lunar_ground()
    layers = ground.layers
    limit = ground.stability_maxdt(DZ, temperatures=(100.0, 420.0))
    for scheme in SCHEMES:
        dt = step_of(scheme, limit)
        t = start(layers)
        cpu = core.Stepper(scheme, stages=3)
        gpu = GpuStepper(ground, layers, FACETS, DZ, scheme=scheme, stages=3, context=ctx)
        assert (gpu.scheme, gpu.stages, gpu.layers, gpu.facets) == (scheme, 3 if scheme == "rkc" else 1, layers, FACETS)
        gpu.set_bottom_flux(0.018)
        gpu.upload(t)
        worst = 0.0
        for k in range(300):
            flux = PEAK * max(numpy.cos(2.0 * numpy.pi * k / 300), 0.0)
            cpu.step(t, flux, ground, dt, DZ, bottom_flux=0.018)
            gpu.step(flux.astype(numpy.float32), dt)
            if (k + 1) % 50 == 0:
                worst = max(worst, numpy.abs(gpu.download() - t).max())
        assert worst < 2e-2, f"{scheme}: {worst} K between the GPU and the CPU"
        assert numpy.abs(gpu.surface() - t[0]).max() < 2e-2
        assert t[0].max() > 300.0


def test_one_material_and_a_surface_given():
    GpuStepper, ctx = gpu_class()
    if GpuStepper is None:
        return
    layers, dz = 20, 2e-3
    limit = core.stability_maxdt(PROP.diffusivity, dz * dz)
    for scheme in SCHEMES:
        dt = step_of(scheme, limit)
        t = start(layers)
        cpu = core.Stepper(scheme, stages=3)
        gpu = GpuStepper(PROP, layers, FACETS, dz, scheme=scheme, stages=3, context=ctx)
        gpu.upload(t)
        for k in range(150):
            top = (250.0 + 50.0 * numpy.sin(0.03 * k + numpy.arange(FACETS))).astype(numpy.float32)
            cpu.step_surface(t, top, PROP, dt, dz)
            gpu.step_surface(top, dt)
        assert numpy.abs(gpu.download() - t).max() < 5e-3, scheme


def test_the_sunlight_computed_on_the_gpu():
    GpuStepper, ctx = gpu_class()
    if GpuStepper is None:
        return
    from kalast.util import AU

    layers, dz, dau = 16, 2e-3, 1.3
    ground = core.Ground(PROP, layers, FACETS)
    ground.albedo = [0.05, 0.1, 0.2, 0.3]
    tilt = 0.6
    normals = numpy.array([[1, 0, 0], [numpy.cos(tilt), numpy.sin(tilt), 0], [-1, 0, 0], [1, 0, 0]], numpy.float32)
    cosi = numpy.array([1.0, numpy.cos(tilt), -1.0, 1.0])
    lit = numpy.array([1.0, 1.0, 1.0, 0.5], numpy.float32)
    flux = numpy.array([core.radiation_sun(dau, c, a) for c, a in zip(cosi, ground.albedo)]) * lit
    dt = 8.0 * ground.stability_maxdt(dz)
    t = core.columns(layers, FACETS, 250.0)
    cpu = core.Stepper("bdf2")
    gpu = GpuStepper(ground, layers, FACETS, dz, scheme="bdf2", context=ctx)
    sun = [float(dau * AU), 0.0, 0.0]
    raises(ValueError, lambda: gpu.step_sun(sun, float(AU), dt), "set_geometry")
    gpu.set_geometry(numpy.zeros((FACETS, 3), numpy.float32), normals)
    gpu.set_lit(lit)
    gpu.upload(t)
    for _ in range(100):
        cpu.step(t, flux, ground, dt, dz)
        gpu.step_sun(sun, float(AU), dt)
    assert numpy.abs(gpu.download() - t).max() < 5e-3
    assert t[0, 0] > t[0, 1] > t[0, 3] > t[0, 2]


def test_a_ground_changed_is_sent_again():
    GpuStepper, ctx = gpu_class()
    if GpuStepper is None:
        return
    layers, dz = 16, 2e-3
    ground = core.Ground(PROP, layers, FACETS)
    dt = 8.0 * ground.stability_maxdt(dz)
    gpu = GpuStepper(ground, layers, FACETS, dz, scheme="crank-nicolson", context=ctx)
    cpu = core.Stepper("crank-nicolson")
    t = start(layers)
    gpu.upload(t)
    flux = PEAK.copy()
    for _ in range(30):
        cpu.step(t, flux, ground, dt, dz)
        gpu.step(flux, dt)
    # A poorer conductor under the first two facets, and the Moon's laws.
    ground.conductivity[:, :2] *= 0.2
    ground.radiative_conductivity = 2.7
    gpu.set_properties(ground)
    for _ in range(30):
        cpu.step(t, flux, ground, dt, dz)
        gpu.step(flux, dt)
    assert numpy.abs(gpu.download() - t).max() < 5e-3


def test_what_the_gpu_cannot_refuse_is_refused_before():
    GpuStepper, ctx = gpu_class()
    if GpuStepper is None:
        return
    layers, dz = 12, 2e-3
    limit = core.stability_maxdt(PROP.diffusivity, dz * dz)
    flux = PEAK.copy()
    gpu = GpuStepper(PROP, layers, FACETS, dz, context=ctx)
    assert repr(gpu) == f"GpuStepper('explicit', {layers} layers, {FACETS} facets)"
    gpu.upload(start(layers))
    raises(ValueError, lambda: gpu.step(flux, 5.0 * limit), "unstable", "RKC takes it in 3 stages")
    raises(ValueError, lambda: gpu.step(flux[:2], 0.5 * limit), "2 absorbed fluxes for 4 facets")
    raises(ValueError, lambda: gpu.step(-flux, 0.5 * limit), "facet 0")
    raises(ValueError, lambda: gpu.upload(start(layers + 1)), "shape")
    raises(ValueError, lambda: gpu.set_bottom_flux(float("nan")), "finite")
    raises(ValueError, lambda: GpuStepper(PROP, layers, FACETS, dz, scheme="leapfrog", context=ctx), "no scheme named")
    raises(ValueError, lambda: GpuStepper(PROP, 2, FACETS, dz, context=ctx), "3 layers")
    raises(TypeError, lambda: GpuStepper("rock", layers, FACETS, dz, context=ctx), "Properties")
    assert numpy.array_equal(gpu.download(), start(layers))


if __name__ == "__main__":
    for name, fn in list(globals().items()):
        if name.startswith("test_") and callable(fn):
            fn()
            print("ok  ", name)
