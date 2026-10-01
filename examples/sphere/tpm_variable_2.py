#!/usr/bin/env python

import numpy

import kalast
from kalast.app import App
from kalast.tpm import core, properties
from kalast.util import AU

app = App()
app.simulation.config.wireframe.mode = 2
app.simulation.config.axes.style = "gizmo"

# Colormap for surface temperature.
app.simulation.config.shading.color_mode = 1
app.simulation.config.data.colormap = "inferno"
app.simulation.config.data.value_min = 180.0
app.simulation.config.data.value_max = 380.0
app.simulation.config.colorbar.enabled = True
app.simulation.config.colorbar.label = "Surface temperature (K)"
app.simulation.config.colorbar.min_max = True
app.simulation.config.colorbar.ticks = 10
app.simulation.config.colorbar.min_max_format = ".0f"

dau = 1.0  # distance to the Sun (AU)
app.simulation.sun.pos = [dau * AU, 0.0, 0.0]
app.simulation.camera.pos = [0.0, 14.5, 5.0]
app.simulation.camera.look_anchor()

app.simulation.load_mesh(path="res/ico4.obj")
mesh = app.simulation.bodies[0].mesh
nf = len(mesh.facets)

# Three materials: loose dust, compact ground, and what both become at depth.
dust = properties.Properties(
    albedo=0.1,
    emissivity=0.9,
    density=1200.0,  # kg/m3
    heat_capacity=600.0,  # J/kg/K
    thermal_inertia=60.0,  # J/m2/K/s^0.5
)
compact = properties.Properties(
    albedo=0.1,
    emissivity=0.9,
    density=1800.0,  # kg/m3
    heat_capacity=600.0,  # J/kg/K
    thermal_inertia=250.0,  # J/m2/K/s^0.5
)
deep = properties.Properties(
    albedo=0.1,
    emissivity=0.9,
    density=2000.0,  # kg/m3
    heat_capacity=600.0,  # J/kg/K
    thermal_inertia=300.0,  # J/m2/K/s^0.5
)
for p in (dust, compact, deep):
    p.compute_conductivity_diffusivity()

# The column: layers thin enough for the dust's daily wave -- six a skin
# depth, which keeps the steps down -- and deep enough for the deep
# material's, which reaches further.
period = 6.0 * 3600.0  # spin period (s)
dz = properties.skin_depth_1(dust.diffusivity, period) / 6.0  # layer thickness (m)
depth = properties.skin_depth_2pi(deep.diffusivity, period)  # column depth (m)
nz = round(depth / dz) + 1  # layers

# Where the dust lies: smooth ponds of it, as on Eros, each centred at a
# latitude and longitude, fading out past its radius -- in degrees.
centres = numpy.array([f.pos for f in mesh.facets])
up = centres / numpy.linalg.norm(centres, axis=1, keepdims=True)


def distance(lat, lon):
    lat, lon = numpy.radians(lat), numpy.radians(lon)
    centre = [numpy.cos(lat) * numpy.cos(lon), numpy.cos(lat) * numpy.sin(lon), numpy.sin(lat)]
    return numpy.degrees(numpy.arccos(numpy.clip(up @ centre, -1.0, 1.0)))


ponds = [(25.0, 0.0, 50.0), (-20.0, 90.0, 10.0), (10.0, 200.0, 15.0), (-35.0, 280.0, 30.0), (50.0, 140.0, 20.0)]
fraction = numpy.zeros(nf)  # how much of a facet is dust: 1 at a pond's centre
for lat, lon, radius in ponds:
    fraction = numpy.maximum(fraction, numpy.exp(-((distance(lat, lon) / radius) ** 2)))

# Each facet's surface, between compact ground and dust, and how deep its
# loose layer goes: H, the depth over which it turns into the deep material.
surface_density = compact.density + (dust.density - compact.density) * fraction
surface_conductivity = compact.conductivity + (dust.conductivity - compact.conductivity) * fraction
h = 0.005 + (0.04 - 0.005) * fraction  # (m)

# Down each column, from its own surface to the deep material over its own H,
# as regolith compacts (Hayne et al. 2017): every layer under every facet.
z = dz * numpy.arange(nz)[:, None]  # depth of each layer (m)
loose = numpy.exp(-z / h)
ground = core.Ground(dust, nz, nf)
ground.density = deep.density + (surface_density - deep.density) * loose
ground.conductivity = deep.conductivity + (surface_conductivity - deep.conductivity) * loose

# Time: the largest step stable in every layer -- the deep material's, the
# most diffusive -- dividing a spin evenly.
steps_per_spin = int(numpy.ceil(period / ground.stability_maxdt(dz)))
dt = period / steps_per_spin  # time step (s)

print(
    f"{nz} layers of {dz * 1000:.2f} mm, column {depth * 100:.1f} cm; "
    f"dt {dt:.1f} s, {steps_per_spin} steps a spin; "
    f"{(fraction > 0.5).sum()} facets mostly dust"
)

# Temperature of each layer under each facet (K), all starting from the mean
# the Sun gives a sphere.
temperature = core.columns(nz, nf, core.effective_temperature(dau, 0.25, dust.albedo, dust.emissivity))

spin_axis = numpy.array([0.0, 0.0, 1.0])

while app.running:
    if app.simulation.state.is_paused:
        app.step()
        continue

    t = app.simulation.state.iteration * dt

    bod = app.simulation.bodies[0]
    bod.mat[:3, :3] = kalast.util.mat_axis_angle(spin_axis, 2.0 * numpy.pi * t / period)

    cosi = app.simulation.facet_incidence(0)
    core.solar_bc(temperature, dau, cosi, ground, dz)
    core.bottom_adiabatic(temperature)
    core.heat_conduction(temperature, ground, dt, dz)

    bod.mesh.values = temperature[0]

    app.step()
