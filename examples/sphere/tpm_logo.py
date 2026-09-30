#!/usr/bin/env python

import numpy

import kalast
from kalast import astro
from kalast.app import App
from kalast.tpm import core, properties
from kalast.util import AU


# Kalast logo is the surface temperature of a sphere resized to Didymos extents
# with its obliquity, spinning through the seasons of its orbit -- a southern
# winter in its fourth year, near aphelion -- using inferno colormap from 90
# to 270 K.

app = App()
app.simulation.config.axes.style = "off"
app.simulation.config.shading.color_mode = 1
app.simulation.config.data.colormap = "inferno"
app.simulation.config.data.value_min = 90.0
app.simulation.config.data.value_max = 270.0

app.simulation.load_mesh(path="res/sph1.obj")
extents = numpy.diag([0.4095, 0.4005, 0.3035])
nf = len(app.simulation.bodies[0].mesh.facets)

# Example materials, not the best Didymos parameters: a high thermal inertia,
# a pole cooling slowly into its winter, is what looks like the logo.
prop = properties.Properties(
    albedo=0.1,
    emissivity=0.9,
    density=2100.0,  # kg/m3
    heat_capacity=600.0,  # J/kg/K
    thermal_inertia=800.0,  # J/m2/K/s^0.5
)
prop.compute_conductivity_diffusivity()

# Didymos's day, and its orbit around the Sun, perihelion on +x. The column is
# graded: thin at the surface for the daily wave, thicker with depth, down past
# the yearly one.
day = 8136.0  # spin period (s), 2.26 h
orbit = astro.Orbit(a=1.6426, e=0.3832)  # AU
year = orbit.period  # (s), 770 days
dz = properties.skin_depth_1(prop.diffusivity, day) / 4.0  # top layers (m)
depth = properties.skin_depth_2pi(prop.diffusivity, year)  # column depth (m)
ground = core.Ground.graded(prop, nf, dz, depth)  # each layer 20 % thicker
dt = ground.stability_maxdt(dz)  # time step (s), about 80
steps_per_frame = round(day / dt)  # a frame a spin

# From 0 K, three full orbits and on into the fourth, held at the southern
# winter solstice near aphelion, the Sun 18 degrees north: the logo.
temperature = core.columns(ground.layers, nf, 0.0)
years = 3.31
app.simulation.state.pause_after_iteration = round(years * year / (steps_per_frame * dt)) - 1

# Didymos's obliquity: its spin axis 18 degrees from the orbit's pole, turning
# backwards, leaning 330 degrees round from perihelion.
spin_axis = numpy.array([0.0, 0.0, 1.0])
obliquity = 162.0  # degrees
lean = kalast.util.mat_axis_angle(spin_axis, numpy.radians(330.0))
tilt = lean @ kalast.util.mat_axis_angle(numpy.array([0.0, 1.0, 0.0]), numpy.radians(obliquity))
bod = app.simulation.bodies[0]

# The logo's view, measured on it: 59 degrees off the south pole, the pole
# low on the left.
app.simulation.camera.pos = [0.47, 1.32, 0.78]
app.simulation.camera.look_anchor()
app.simulation.camera.up = [-0.040, 0.522, -0.852]

step = 0
while app.running:
    if app.simulation.state.is_paused:
        app.step()
        continue

    for _ in range(steps_per_frame):
        t = step * dt

        pos = orbit.position(t)  # (AU)
        dau = numpy.linalg.norm(pos)  # distance to the Sun (AU)
        app.simulation.sun.pos = -pos * AU
        spin = kalast.util.mat_axis_angle(spin_axis, 2.0 * numpy.pi * t / day)
        bod.mat[:3, :3] = tilt @ spin @ extents

        cosi = app.simulation.facet_incidence(0)
        core.solar_bc(temperature, dau, cosi, ground, dz)
        core.bottom_adiabatic(temperature)
        core.heat_conduction(temperature, ground, dt, dz)
        step += 1

    bod.mesh.values = temperature[0]

    app.step()
