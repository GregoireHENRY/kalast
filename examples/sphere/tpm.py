#!/usr/bin/env python

import pathlib

import numpy

import kalast
from kalast.app import App
from kalast.tpm import core, properties
from kalast.util import AU

app = App()
app.simulation.config.shading.color_mode = 1
app.simulation.config.data.colormap = "inferno"
app.simulation.config.data.value_min = 120.0
app.simulation.config.data.value_max = 380.0
app.simulation.config.colorbar.enabled = True
app.simulation.config.colorbar.label = "Surface temperature (K)"
app.simulation.config.colorbar.min_max = True
app.simulation.config.colorbar.ticks = 10

dau = 1.0  # distance to the Sun (AU)
app.simulation.sun.pos = [dau * AU, 0.0, 0.0]
app.simulation.camera.pos = [13.5, -6.0, 5.0]
app.simulation.camera.look_anchor()

app.simulation.load_mesh(path="res/ico4.obj")
nf = len(app.simulation.bodies[0].mesh.facets)

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

# Temperature of each layer under each facet (K), each column starting at its
# latitude's effective temperature: from the sunlight it gets on average over
# a spin about the body's z axis.
cosi = app.simulation.facet_mean_incidence(0)
temperature = core.columns(nz, nf, core.effective_temperature(dau, cosi, prop.albedo, prop.emissivity))

spin_axis = numpy.array([0.0, 0.0, 1.0])

# For tpm_plot.py: the columns under the meridian at longitude 0, pole to
# pole, 24 times a spin, saved in out/sphere/ when the run stops.
meridian, latitudes = app.simulation.meridian_facets(0)
history = []
n_export_per_spin = 24

while app.running:
    if app.simulation.state.is_paused:
        app.step()
        continue

    t = app.simulation.state.iteration * dt

    bod = app.simulation.bodies[0]
    bod.mat[:3, :3] = kalast.util.mat_axis_angle(spin_axis, 2.0 * numpy.pi * t / period)

    cosi = app.simulation.facet_incidence(0)
    core.solar_bc(temperature, dau, cosi, prop, dz)
    core.bottom_adiabatic(temperature)
    core.heat_conduction(temperature, prop, dt, dz)

    bod.mesh.values = temperature[0]

    # Export every 24th of a spin.
    if app.simulation.state.iteration * n_export_per_spin % steps_per_spin < n_export_per_spin:
        history.append((t, temperature[:, meridian].astype(numpy.float32)))

    app.step()

if history:
    times, columns = zip(*history)
    pathlib.Path("out/sphere").mkdir(parents=True, exist_ok=True)
    numpy.savez(
        f"out/sphere/{pathlib.Path(__file__).stem}.npz",
        time=times,
        temperature=columns,
        latitude=latitudes,
        depth=dz * numpy.arange(nz),
        period=period,
        inertia=prop.thermal_inertia,
        albedo=prop.albedo,
    )
