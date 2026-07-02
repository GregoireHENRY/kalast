#!/usr/bin/env python

import math
import time
from pathlib import Path  # noqa

import glm  # noqa
import numpy
import trimesh
from matplotlib import pyplot
# import matplotlib

import kalast
from kalast.config import Config
from kalast.util import AU, HOUR, DAY, RPD, matpow, numdigits_comma, STEFAN_BOLTZMANN  # noqa
from kalast.astro import Body, Column
from kalast.tpm.core import (
    stability_maxdt,
    stability,
    skin_depth_1,
    skin_depth_2pi,
)
from kalast.props import PROPERTIES


# General config.
cfg = Config()
cfg.run()

# Sun position in global reference frame.
sun0 = numpy.array([1.0, 0, 0]) * AU
sun = sun0.copy()
dau = 1.0
dsun = AU

# Body information.
body = Body()
spin_period = 12 * HOUR
orbit_period = 365 * 2 * HOUR

# Surface properties.
body.surf.mesh = trimesh.Trimesh(
    vertices=[[100.0, 0.0, 0.0]], vertex_normals=[[1.0, 0.0, 0.0]]
)
prop = PROPERTIES["didymos"]
body.surf.set_props_constant_face(prop)

se = STEFAN_BOLTZMANN * prop.e
print(f"k={prop.e:.6e} d={prop.d:.6e}")

# Interior, properties, initial temperatures.
body.inte = Column()
dx0 = 1e-2
twodx0 = 2 * dx0
dx02 = dx0 * dx0
ls1 = skin_depth_1(prop.d, spin_period)
ls2pi = skin_depth_2pi(prop.d, spin_period)
ls2pi_orb = skin_depth_2pi(prop.d, orbit_period)
maxdepth = ls2pi
body.inte.z = numpy.arange(0, maxdepth + dx0, dx0)
nx = body.inte.z.size
nx_ls1 = (body.inte.z <= ls1).sum()
nx_ls2pi = (body.inte.z <= ls2pi).sum()
nx_save = (body.inte.z <= 4 * ls1).sum()
# nx_save = nx
dx = numpy.diff(body.inte.z)
dx2in = dx[:-1] * dx[:-1]
body.inte.p = numpy.ones(nx) * prop.p
body.inte.c = numpy.ones(nx) * prop.c
body.inte.k = numpy.ones(nx) * prop.k
body.inte.d = numpy.ones(nx) * prop.d
body.tmp = numpy.ones(nx) * 290.0
print(
    f"dx={dx0:.4f} ls1={ls1:.4f}({nx_ls1}) ls2pi={ls2pi:.4f}({nx_ls2pi}) ls2pi_orb={ls2pi_orb:.4f} maxdepth={maxdepth:.4f}({nx})"
)

# Time.
dt = 300
tf = 20 * DAY
nit = numpy.ceil(tf / dt).astype(int) + 1
S = stability(prop.d, dt, dx02)
print(f"Using dt={dt}, stability={S:.2f}")
print(f"simulation time={tf / DAY}days, {nit} it")

# Check convergence.
maxdt = stability_maxdt(dx02, prop.d)
print(f"max dt stable: {maxdt:.2f}")
if S > 0.5:
    raise ValueError("Stability criteria not valid.")

# Time loop progress.
progress_freq = "10"
digits = [len(_d) for _d in progress_freq.split(".")]
digits_full = 3
digits_decimal = 0
if len(digits) == 2:
    digits_decimal = digits[1]
    if digits_decimal > 0:
        digits_full += digits_decimal + 1
freqv = float(progress_freq)
last_freq_reached = -freqv
ndigits = numdigits_comma(freqv)
digit = 10**ndigits

# Saving.
t_save = 24 * HOUR
nii_save = t_save // dt
nii_hour = HOUR // dt
ts = numpy.zeros(nii_save)
tmp = numpy.zeros((nii_save, nx))
print(f"{nii_save} iterations will be recorded (frequence update: {progress_freq}%)")
print()

# to update if dt changes.
m_spin = matpow(body.spin, dt)
dtpdx2in = dt / dx2in

# Loop variables.
t = 0
it = 0

it_rot = 2.0 * numpy.pi * dt / spin_period

while True:
    # Get body orientation and position wrt Sun
    sun = sun0 * numpy.array([numpy.cos(it_rot * it), numpy.sin(it_rot * it), 0.0])

    # For all facets, get incidence angle and distance of Sun
    p = body.surf.mesh.vertices[0]
    n = body.surf.mesh.vertex_normals[0]

    u_sun = sun / dsun
    cosi = kalast.cosine_incidence(u_sun, n)

    # Get surface flux
    sflux = kalast.radiation_sun(dau, cosi, prop.a)

    # Conduction of temperature
    body.tmp[0] = kalast.newton_method(
        body.tmp[0], sflux, se, prop.k, body.tmp[1], body.tmp[2], twodx0
    )
    body.tmp[1:-1] = kalast.conduction_1d(body.tmp, body.inte.d, dtpdx2in)
    body.tmp[-1] = body.tmp[-2]
    if body.tmp[0] is None:
        raise ValueError("Newton method never converged.")

    # Save data
    if nit - it <= nii_save:
        ii_save = nii_save - nit + it
        ts[ii_save] = t
        tmp[ii_save] = body.tmp

    # Show progress
    progress = it / (nit - 1) * 100
    if ndigits > 0:
        progress = numpy.floor(progress * digit) / digit
    if progress >= last_freq_reached + freqv:
        last_freq_reached += freqv
        print(f"{progress:{digits_full}.{digits_decimal}f}% ({it:,}/{nit - 1:,}it)")

    # Update loop
    if t >= tf:
        break
    t += dt
    it += 1
    if it == 1:
        timer_1 = time.perf_counter()

# Final show progress
if last_freq_reached < 100:
    print(f"{100:{digits_full}.{digits_decimal}f}% ({it:,}/{nit - 1:,}it)")
print()
timer_2 = time.perf_counter()
timer_elapsed = timer_2 - timer_1
print(
    f"Simulation duration = {timer_elapsed:.4f}s ({math.floor((nit - 1) / timer_elapsed):,}it/s)"
)

# Prepare plot
ts -= ts[0]
ts /= HOUR

kalast.plot.style.load()
fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Hours elapsed [h]")
ax.set_ylabel("Temperature [K]")
ax.plot(ts, tmp[:, 0], lw=1, color="k")
ax.set_xlim(0, t_save / HOUR)
# ax.set_ylim(0, None)
# ax.set_yscale("log")
# pyplot.legend()
fig.savefig("surf.png", bbox_inches="tight", dpi=300)
# pyplot.show()

fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Temperature [K]")
ax.set_ylabel("Depth [cm]")
for ii in range(0, nii_save // 2, nii_hour):
    ax.plot(tmp[ii, :], body.inte.z * 100, lw=1, color="k")
# ax.set_xlim(0, None)
ax.set_ylim(0, body.inte.z[nx_save - 1] * 100)
ax.invert_yaxis()
fig.savefig("depth_zoom.png", bbox_inches="tight", dpi=300)

ax.set_ylim(body.inte.z[-1] * 100, 0)
fig.savefig("depth_full.png", bbox_inches="tight", dpi=300)
pyplot.show()
