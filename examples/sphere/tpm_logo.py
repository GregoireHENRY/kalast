#!/usr/bin/env python

import pathlib
import time
from collections import deque

import numpy

import kalast
from kalast import astro
from kalast.app import App, Hud
from kalast.tpm import core, properties
from kalast.util import AU


# Kalast logo is the surface temperature of a sphere resized to Didymos extents
# with its obliquity, spinning through the seasons of its orbit using inferno
# colormap from 90 to 270 K.
# The view is looking at southern winter near aphelion after 30 years of
# simulation, when the temperatures have settled.
# Toggle off the wireframe and you have kalast logo.

app = App()
app.simulation.config.shading.color_mode = 1
app.simulation.config.data.colormap = "inferno"
app.simulation.config.data.value_min = 90.0
app.simulation.config.data.value_max = 270.0
app.simulation.huds = [Hud("", size=20, anchor="top-right", align_h="right", x=100)]

app.simulation.load_mesh(path="res/sph1.obj")
extents = numpy.diag([0.4095, 0.4005, 0.3035])
nf = len(app.simulation.bodies[0].mesh.facets)

# Example materials, not the best Didymos parameters.
prop = properties.Properties(
    albedo=0.1,
    emissivity=0.9,
    density=2100.0,  # kg/m3
    heat_capacity=600.0,  # J/kg/K
    thermal_inertia=500.0,  # J/m2/K/s^0.5
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
max_dt = ground.stability_maxdt(dz)
steps_per_spin = int(numpy.ceil(day / max_dt))  # 101
dt = day / steps_per_spin  # time step (s), about 80

# Rendering one frame can be more expensive than one TPM iteration.
# One frame can be rendered after multiple TPM steps to reach end of simulation
# faster.
steps_per_frame = steps_per_spin  # 1 frame = 1 spin
# steps_per_frame = 1  # 1 frame = 1 TPM step

# 30 orbits for the columns, 11 m deep, to settle, and 0.31 more for the
# logo's view.
years = 30.31
app.simulation.state.pause_after_iteration = round(years * year / (steps_per_frame * dt)) - 1

# Didymos's obliquity: its spin axis 18 degrees from the orbit's pole, turning
# backwards, leaning 330 degrees round from perihelion.
spin_axis = numpy.array([0.0, 0.0, 1.0])
obliquity = 162.0  # degrees
lean = kalast.util.mat_axis_angle(spin_axis, numpy.radians(330.0))
tilt = lean @ kalast.util.mat_axis_angle(numpy.array([0.0, 1.0, 0.0]), numpy.radians(obliquity))
bod = app.simulation.bodies[0]

# Each column starts at the temperature its facet's mean sunlight over an
# orbit holds it at: above the yearly mean it settles to, but far nearer than
# 0 K. The mean at 100 points of the orbit, 24 turns of the spin at each.
sunlight = numpy.zeros(nf)  # cos(i) / distance^2 (AU), averaged
for t in numpy.linspace(0.0, year, 100, endpoint=False):
    pos = orbit.position(t)
    app.simulation.sun.pos = -pos * AU
    for turn in numpy.linspace(0.0, 2.0 * numpy.pi, 24, endpoint=False):
        bod.mat[:3, :3] = tilt @ kalast.util.mat_axis_angle(spin_axis, turn) @ extents
        sunlight += app.simulation.facet_incidence(0) / (pos @ pos) / 2400
temperature = core.columns(ground.layers, nf, core.effective_temperature(1.0, sunlight, prop.albedo, prop.emissivity))

# The logo's view, measured on it: 59 degrees off the south pole, the pole
# low on the left.
app.simulation.camera.pos = [0.35, 1.5, 0.9]
app.simulation.camera.look_anchor()
app.simulation.camera.up = [-0.04, 0.5, -0.85]

# For tpm_plot.py: the columns under the meridian at longitude 0, pole to
# pole, every 10 spins, and 24 times a spin through the last two, saved in
# out/sphere/ when the run stops.
bod.mat[:3, :3] = tilt @ extents  # the shape, as the pose stretches it
meridian, latitudes = app.simulation.meridian_facets(0)
# The graded layers' widths from the conductivity they carry, k dz / w.
widths = dz * prop.conductivity / ground.conductivity[:, 0]
history = []
last_spins = deque(maxlen=48)

step = 0
t = 0.0  # simulated time (s), for the HUD before the first step
frames = deque([(time.perf_counter(), 0)])  # (time, step) a frame, back one second
while app.running:
    # TPM it/s: the steps done in the last second. Before the pause, so it
    # falls to 0 a second after one.
    now = time.perf_counter()
    frames.append((now, step))
    while frames[1][0] <= now - 1.0:
        frames.popleft()
    its = step - frames[0][1]
    app.simulation.huds[0].text = f"TPM it/s={its} progress={t/year:.2f}/{years}years"

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
        if step * 24 % steps_per_spin < 24:
            last_spins.append((t, temperature[:, meridian].astype(numpy.float32)))
        step += 1

    bod.mesh.values = temperature[0]

    if step % (10 * steps_per_spin) < steps_per_frame:
        history.append((t, temperature[:, meridian].astype(numpy.float32)))

    app.step()

if history:
    times, columns = zip(*history)
    spin_times, spin_columns = zip(*last_spins)
    pathlib.Path("out/sphere").mkdir(parents=True, exist_ok=True)
    numpy.savez(
        f"out/sphere/{pathlib.Path(__file__).stem}.npz",
        time=times,
        temperature=columns,
        spin_time=spin_times,
        spin_temperature=spin_columns,
        latitude=latitudes,
        depth=numpy.concatenate([[0.0], numpy.cumsum((widths[:-1] + widths[1:]) / 2.0)]),
        period=day,
        year=year,
        obliquity=obliquity,
        inertia=prop.thermal_inertia,
        albedo=prop.albedo,
    )
