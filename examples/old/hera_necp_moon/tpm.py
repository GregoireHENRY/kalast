#!/usr/bin/env python

import copy
import math
import time
from pathlib import Path  # noqa

import pandas  # noqa
import glm
import numpy
import trimesh
import spiceypy as spice

import kalast
from kalast.config import Config
from kalast.util import (  # noqa
    AU,
    HOUR,
    DAY,
    DPR,
    RPD,
    matpow,
    numdigits_comma,
    STEFAN_BOLTZMANN,
)
from kalast.astro import Body, matspin, matobliq, cosinc, Column  # noqa
from kalast.tpm.core import (
    solar_radiation,
    newton_method,
    conduction_1d,
    stability_maxdt,
    stability,
    skin_depth_1,
    skin_depth_2pi,
)
from kalast.props import PROPERTIES
from kalast.spice_entities import earth, moon  # noqa


cfg = Config()
cfg.run()

spice.kclear()
# spice.furnsh("/Users/gregoireh/data/spice/wgc/mk/solar_system_v0060.tm")
spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops_local.tm")
frame = "j2000"

bod = moon

date_start = "2024-06-01 00:00"
date_start_export = "2024-10-10 00:00"
date_stop = "2024-10-25 00:00"

et_start = spice.str2et(date_start)
et_start_export = spice.str2et(date_start_export)
et_stop = spice.str2et(date_stop)

# Body information.
body = Body()
spin_period = moon.spin_period
solar_orbit_period = kalast.spice_entities.earth.orbit_period

# Thermal properties.
prop = PROPERTIES[moon.name.lower()]
se = STEFAN_BOLTZMANN * prop.e
print(f"k={prop.k:.6e} d={prop.d:.6e}")

# Surface.
body.surf.mesh = trimesh.load("in/moon.obj")
nface = body.surf.nfaces()
nvert = body.surf.nvertices()
body.surf.set_face_props_constant(prop)
sph = numpy.array([kalast.util.cart2sph(v) for v in body.surf.mesh.triangles_center])
print(f"nfaces={nface} nvert={nvert}")

equator = numpy.load("work/scene/equator.npy")
meridian0 = numpy.load("work/scene/meridian0.npy")
equator_meridian0 = numpy.load("work/scene/equator_meridian0.npy")

# Interior, properties, initial temperatures.
c = Column()
dx0 = 2e-3
twodx0 = 2 * dx0
dx02 = dx0 * dx0
ls1 = skin_depth_1(prop.d, spin_period)
ls2pi = skin_depth_2pi(prop.d, spin_period)
ls2pi_orb = skin_depth_2pi(prop.d, solar_orbit_period)
maxdepth = ls2pi
c.z = numpy.arange(0, maxdepth + dx0, dx0)
nx = c.z.size
nx_ls1 = (c.z <= ls1).sum()
nx_ls2pi = (c.z <= ls2pi).sum()
nx_save = (c.z <= 4 * ls1).sum()
# nx_save = nx
dx = numpy.diff(c.z)
dx2in = dx[:-1] * dx[:-1]
c.t = numpy.ones(nx) * 100.0
c.p = numpy.ones(nx) * prop.p
c.c = numpy.ones(nx) * prop.c
c.k = numpy.ones(nx) * prop.k
c.d = numpy.ones(nx) * prop.d
print(
    f"dx={dx0:.4f} ls1={ls1:.4f}({nx_ls1}) ls2pi={ls2pi:.4f}({nx_ls2pi}) ls2pi_orb={ls2pi_orb:.4f} maxdepth={maxdepth:.4f}({nx})"
)

for ii in range(0, nface):
    body.inte.append(copy.deepcopy(c))

# Time.
dt = 300  # 30 120 300
tf = et_stop - et_start
t_save = et_stop - et_start_export
nit = numpy.ceil(tf / dt).astype(int) + 1
nii_save = numpy.ceil(t_save / dt).astype(int) + 1

S = stability(prop.d, dt, dx02)
print(
    f"Using dt={dt}, stability={S:.2f}, spin={spin_period / 3600:.3f}h({spin_period // dt:.0f})"
)
print(f"simulation time: {tf / DAY:.3}days ({nit}it)")

nii_hour = HOUR // dt
nii_day = DAY // dt

# Check convergence.
maxdt = stability_maxdt(dx02, prop.d)
print(f"max dt stable: {maxdt:.2f}")
if S > 0.5:
    raise ValueError("Stability criteria not valid.")

# Time loop progress.
progress_freq = "1"
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
save_face = equator_meridian0[0]
et_save = numpy.zeros(nii_save)
sun_save = numpy.zeros((nii_save, 3))
tmp_surf_save = numpy.zeros((nii_save, nface))
tmp_cols_save = numpy.zeros((nii_save, nx))
print(f"Recording: {nii_save}it (update: {progress_freq}%)")
print()

# to update if dt changes.
dtpdx2in = dt / dx2in

# Loop variables.
t = 0
it = 0
it_save = 0
exporting = False

while True:
    et = et_start + t

    (sun, _lt) = spice.spkpos("sun", et, bod.frame, "none", bod.name)
    sun *= 1e3

    # Prepare save data
    if not exporting and et >= et_start_export:
        exporting = True
        print("Recording started.")

    if exporting:
        et_save[it_save] = et

    for ii in range(0, nface):
        p = body.surf.mesh.triangles_center[ii]
        n = body.surf.mesh.face_normals[ii]

        v_sun = sun - p
        d_sun = glm.length(v_sun)
        dau_sun = d_sun / AU
        u_sun = v_sun / d_sun
        cosi = cosinc(u_sun, n)

        # Get surface flux
        sflux = solar_radiation(dau_sun, cosi, body.surf.a[ii])

        # Conduction of temperature
        body.inte[ii].t[0] = newton_method(
            body.inte[ii].t[0],
            sflux,
            se,
            body.inte[ii].k[0],
            body.inte[ii].t[1:3],
            twodx0,
        )
        body.inte[ii].t[1:-1] = conduction_1d(
            body.inte[ii].t, body.inte[ii].d, dtpdx2in
        )
        body.inte[ii].t[-1] = body.inte[ii].t[-2]
        if body.inte[ii].t[0] is None:
            raise ValueError("Newton method never converged.")

    # Save data
    if exporting:
        tmp_surf_save[it_save] = numpy.array([c.t[0] for c in body.inte])
        tmp_cols_save[it_save] = body.inte[save_face].t
        sun_save[it_save] = sun

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

    if exporting:
        it_save += 1

    if it == 1:
        timer_1 = time.perf_counter()

# Final show progress
if last_freq_reached < 100:
    print(f"{100:{digits_full}.{digits_decimal}f}% ({it:,}/{nit - 1:,}it)")
print()
timer_2 = time.perf_counter()
timer_elapsed = timer_2 - timer_1
print(
    f"Simulation duration: {timer_elapsed:.4f}s ({math.floor((nit - 1) / timer_elapsed):,}it/s)"
)
print(
    f"Avg surf temps: mean={tmp_surf_save.mean():.2f} min={tmp_surf_save.min():.2f} max={tmp_surf_save.max():.2f}"
)
print(f"Record completed ({nii_save}it)")

et = et_save
z = body.inte[save_face].z

n_saved = it_save + 1
et = et[:n_saved]
sun_save = sun_save[:n_saved]
tmp_surf_save = tmp_surf_save[:n_saved]
tmp_cols_save = tmp_cols_save[:n_saved]

tmp_state_save = numpy.zeros((len(body.inte), len(body.inte[0].t)))
for ii in range(len(body.inte)):
    tmp_state_save[ii] = body.inte[ii].t

nbytes = 0
nbytes += et.nbytes
nbytes += z.nbytes
nbytes += sun_save.nbytes
nbytes += tmp_surf_save.nbytes
nbytes += tmp_cols_save.nbytes
nbytes += tmp_state_save.nbytes
print(f"Exporting {nbytes / 1e6:.3f}MB of data")

numpy.save("et_simu.npy", et)
numpy.save("z.npy", z)
numpy.save("sun_simu.npy", sun_save)
numpy.save("tmp_surf.npy", tmp_surf_save)
numpy.save("tmp_cols.npy", tmp_cols_save)
numpy.save("tmp_state.npy", tmp_state_save)
