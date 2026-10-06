#!/usr/bin/env python

import pathlib

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
app.simulation.config.data.value_min = 200.0
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

# Two materials: a fine regolith at the surface, over rock.
regolith = properties.Properties(
    albedo=0.1,
    emissivity=0.9,
    density=1500.0,  # kg/m3
    heat_capacity=600.0,  # J/kg/K
    thermal_inertia=150.0,  # J/m2/K/s^0.5
)
rock = properties.Properties(
    albedo=0.1,
    emissivity=0.9,
    density=2500.0,  # kg/m3
    heat_capacity=600.0,  # J/kg/K
    thermal_inertia=600.0,  # J/m2/K/s^0.5
)
regolith.compute_conductivity_diffusivity()
rock.compute_conductivity_diffusivity()

# The column: layers thin enough for the regolith's daily wave, deep enough
# for the rock's, which reaches further down.
period = 6.0 * 3600.0  # spin period (s)
dz = properties.skin_depth_1(regolith.diffusivity, period) / 8.0  # layer thickness (m)
depth = properties.skin_depth_2pi(rock.diffusivity, period)  # column depth (m)
nz = round(depth / dz) + 1  # layers
regolith_depth = 0.01  # regolith thickness (m), under its skin depth: the rock shows through
nr = round(regolith_depth / dz)  # regolith layers

# Every facet regolith over rock...
ground = core.Ground(regolith, nz, nf)
ground.conductivity[nr:] = rock.conductivity
ground.density[nr:] = rock.density
ground.heat_capacity[nr:] = rock.heat_capacity

# ...but in circular areas, centred at a latitude and longitude, radius in
# degrees: the facets whose centre is within it, seen from the body's centre.
centres = numpy.array([f.pos for f in mesh.facets])
up = centres / numpy.linalg.norm(centres, axis=1, keepdims=True)


def area(lat, lon, radius):
    lat, lon = numpy.radians(lat), numpy.radians(lon)
    centre = [numpy.cos(lat) * numpy.cos(lon), numpy.cos(lat) * numpy.sin(lon), numpy.sin(lat)]
    return up @ centre >= numpy.cos(numpy.radians(radius))


# A brighter patch, fresh material: less sunlight absorbed.
bright = area(30.0, 0.0, 25.0)
ground.albedo[bright] = 0.3

# Bare rock, no regolith: rock up to the surface, all the way down.
bare = area(-20.0, 120.0, 30.0)
ground.conductivity[:, bare] = rock.conductivity
ground.density[:, bare] = rock.density
ground.heat_capacity[:, bare] = rock.heat_capacity

# Time: the largest step stable in every layer -- the rock's, the more
# diffusive -- dividing a spin evenly.
steps_per_spin = int(numpy.ceil(period / ground.stability_maxdt(dz)))
dt = period / steps_per_spin  # time step (s)

print(
    f"{nz} layers of {dz * 1000:.2f} mm, {nr} of regolith; column {depth * 100:.1f} cm; "
    f"dt {dt:.1f} s, {steps_per_spin} steps a spin; "
    f"{bright.sum()} facets bright, {bare.sum()} bare rock"
)

# Temperature of each layer under each facet (K), all starting from the mean
# the Sun gives a sphere.
temperature = core.columns(nz, nf, core.effective_temperature(dau, 0.25, regolith.albedo, regolith.emissivity))

spin_axis = numpy.array([0.0, 0.0, 1.0])

# For tpm_plot.py: the columns under the meridian at longitude 0, pole to
# pole, 24 times a spin, saved in out/sphere/ when the run stops.
meridian, latitudes = app.simulation.meridian_facets(0)
history = []

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

    if app.simulation.state.iteration * 24 % steps_per_spin < 24:
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
        inertia=numpy.sqrt(ground.conductivity * ground.density * ground.heat_capacity)[:, meridian],
        albedo=ground.albedo[meridian],
    )
