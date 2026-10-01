#!/usr/bin/env python

import numpy

import kalast
from kalast.app import App
from kalast.tpm import core, properties
from kalast.util import AU

app = App()
app.simulation.config.shading.color_mode = 1
app.simulation.config.data.colormap = "inferno"
app.simulation.config.data.value_min = 0.0
app.simulation.config.data.value_max = 380.0
app.simulation.config.colorbar.enabled = True
app.simulation.config.colorbar.label = "Surface temperature (K)"
app.simulation.config.colorbar.min_max = True
app.simulation.config.colorbar.ticks = 12

dau = 1.0  # distance to the Sun (AU)
app.simulation.sun.pos = [dau * AU, 0.0, 0.0]

app.simulation.load_mesh(path="res/sph1.obj")
nf = len(app.simulation.bodies[0].mesh.facets)

# The view the gizmo's -Y gives: down y at the xz plane, orthographic, the
# Sun to the right and the tilt seen side on. After the load, which it frames.
app.simulation.camera.view_along("y", positive=False)

# Thermal properties of the surface.
prop = properties.Properties(
    albedo=0.1,
    emissivity=0.9,
    density=2000.0,  # kg/m3
    heat_capacity=600.0,  # J/kg/K
    thermal_inertia=200.0,  # J/m2/K/s^0.5
)
prop.compute_conductivity_diffusivity()

# The spin drives a daily thermal wave into the ground. Its skin depth sets
# the column: layers thin enough to resolve it, and deep enough for it to
# have died out at the bottom.
period = 6.0 * 3600.0  # spin period (s)
skin_depth = properties.skin_depth_1(prop.diffusivity, period)  # (m)
dz = skin_depth / 8.0  # layer thickness (m)
depth = properties.skin_depth_2pi(prop.diffusivity, period)  # column depth (m)
nz = round(depth / dz) + 1  # layers

# Time: the largest stable step that divides a spin evenly, for a few hundred
# spins.
steps_per_spin = int(numpy.ceil(period / core.stability_maxdt(prop.diffusivity, dz**2, 0.5)))
dt = period / steps_per_spin  # time step (s)

print(
    f"skin depth {skin_depth * 100:.2f} cm, column {depth * 100:.1f} cm in {nz} layers, "
    f"dt {dt:.1f} s, {steps_per_spin} steps a spin"
)

spin_axis = numpy.array([0.0, 0.0, 1.0])

# Obliquity: the spin axis tilted from the orbit's normal, here toward the
# Sun -- the northern summer solstice: the Sun overhead at 25 degrees north,
# the north polar cap in daylight all spin long and the south in night.
# Tilted about x rather than y, it would be an equinox instead.
obliquity = 25.0  # degrees
tilt = kalast.util.mat_axis_angle(numpy.array([0.0, 1.0, 0.0]), numpy.radians(obliquity))

# Temperature of each layer under each facet (K), each column starting at its
# latitude's effective temperature: from the sunlight it gets on average over
# a spin, none in the polar night, all spin long in the polar day. Read from
# the pose, so tilted first.
app.simulation.bodies[0].mat[:3, :3] = tilt
cosi = app.simulation.facet_mean_incidence(0)
temperature = core.columns(nz, nf, core.effective_temperature(dau, cosi, prop.albedo, prop.emissivity))

while app.running:
    if app.simulation.state.is_paused:
        app.step()
        continue

    t = app.simulation.state.iteration * dt

    bod = app.simulation.bodies[0]
    bod.mat[:3, :3] = tilt @ kalast.util.mat_axis_angle(spin_axis, 2.0 * numpy.pi * t / period)

    cosi = app.simulation.facet_incidence(0)
    core.solar_bc(temperature, dau, cosi, prop, dz)
    core.bottom_adiabatic(temperature)
    core.heat_conduction(temperature, prop, dt, dz)

    bod.mesh.values = temperature[0]

    app.step()
