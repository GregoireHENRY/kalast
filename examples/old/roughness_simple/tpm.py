#!/usr/bin/env python

import copy
import math
import time
from pathlib import Path  # noqa

import pandas  # noqa
import glm
import numpy
import trimesh

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
from kalast.astro import Body, matobliq, cosinc, Column  # noqa
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


cfg = Config()
cfg.run()

sun = numpy.array([1.0, 0.0, 0.0]) * AU

# Body information.
body = Body()
spin_period = 6.0 * 3600
solar_orbit_period = kalast.spice_entities.didymos.orbit_period

# Time.
dt = 120  # 30 120 300
dt_save = 30
tf = 100.0 * spin_period
t_prep = 98.0 * spin_period
t_save = tf - t_prep
nit_prep = numpy.ceil(t_prep / dt).astype(int) + 1
nit_save = numpy.ceil(t_save / dt_save).astype(int) + 1
nit = nit_prep + nit_save
nit_hour = HOUR // dt
nit_day = DAY // dt
print(f"simulation time: {tf / DAY:.3}days ({nit}it)")

matspin = kalast.astro.matspin(spin_period / dt, [0, 0, 1])
matspin_save = kalast.astro.matspin(spin_period / dt_save, [0, 0, 1])

matspin = numpy.array(matspin)[:3, :3]
matspin_save = numpy.array(matspin_save)[:3, :3]

# Thermal properties.
prop = PROPERTIES["didymos"]
se = STEFAN_BOLTZMANN * prop.e
print(f"k={prop.k:.6e} d={prop.d:.6e}")

# Surface.
body.surf.mesh = trimesh.load("work/mesh.obj")
nface = body.surf.nfaces()
nvert = body.surf.nvertices()
body.surf.set_props_constant_face(prop)
print(f"nfaces={nface} nvert={nvert}")

equator = numpy.load("work/equator.npy")
meridian0 = numpy.load("work/meridian0.npy")
equator_meridian0 = numpy.load("work/equator_meridian0.npy")

# Interior, properties, initial temperatures.
c = Column()
dx0 = 4e-3
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
dtpdx2in = dt / dx2in
dtpdx2in_save = dt_save / dx2in

c.set_temp_constant(200.0)
c.set_props_constant(prop)
print(
    f"dx={dx0:.4f} ls1={ls1:.4f}({nx_ls1}) ls2pi={ls2pi:.4f}({nx_ls2pi}) ls2pi_orb={ls2pi_orb:.4f} maxdepth={maxdepth:.4f}({nx})"
)

for ii in range(0, nface):
    body.inte.append(copy.deepcopy(c))

# Check convergence.
maxdt = stability_maxdt(dx02, prop.d)
print(f"max dt stable: {maxdt:.2f}")
S = stability(prop.d, dt, dx02)
print(
    f"Using dt={dt}, stability={S:.2f}, spin={spin_period / 3600:.3f}h({spin_period // dt:.0f})"
)
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
et_save = numpy.zeros(nit_save)
sun_save = numpy.zeros((nit_save, 3))
tmp_surf_save = numpy.zeros((nit_save, nface))
tmp_cols_save = numpy.zeros((nit_save, nx))
tmp_surf_max = numpy.zeros(nface)
tmp_surf_avg = numpy.zeros(nface)
tmp_surf_min = numpy.zeros(nface)
print(f"Recording: {nit_save}it (update: {progress_freq}%)")
print()

# Loop variables.
t = 0
it = 0
it_save = 0
exporting = False

while True:
    # Prepare save data
    if not exporting and t >= t_prep:
        exporting = True
        dt = dt_save
        matspin = matspin_save
        dtpdx2in = dtpdx2in_save
        print("Recording started.")

    if exporting:
        et_save[it_save] = t

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
        tmp_surf = numpy.array([c.t[0] for c in body.inte])
        tmp_surf_save[it_save] = tmp_surf
        tmp_cols_save[it_save] = body.inte[save_face].t
        sun_save[it_save] = sun

        tmp_surf_max = numpy.max((tmp_surf_max, tmp_surf), axis=0)
        tmp_surf_avg = (tmp_surf_avg * it_save + tmp_surf) / (it_save + 1)
        tmp_surf_min = numpy.min((tmp_surf_min, tmp_surf), axis=0)

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
    sun = matspin @ sun

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
print(f"Record completed ({nit_save}it)")

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

numpy.save("tmp_surf_max.npy", tmp_surf_max)
numpy.save("tmp_surf_avg.npy", tmp_surf_avg)
numpy.save("tmp_surf_min.npy", tmp_surf_min)
