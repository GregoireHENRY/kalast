#!/usr/bin/env python

import time
from pathlib import Path  # noqa

import pandas
import numpy
import spiceypy as spice

import kalast
from kalast.util import (  # noqa
    AU,
    HOUR,
    DAY,
    DPR,
    RPD,
    STEFAN_BOLTZMANN,
)

# Spice setup
spice.kclear()
# spice.furnsh("/Users/gregoireh/data/spice/wgc/mk/solar_system_v0060.tm")
spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops_local.tm")
frame = "j2000"

# Body
deimos = kalast.entity.DEIMOS
deimos.solar_orbit_period = kalast.entity.MARS.orbit_period

# Surface
mesh = kalast.mesh.Mesh(
    "/Users/gregoireh/data/spice/hera/kernels/dsk/deimos_k005_tho_v02.obj"
)
nface = len(mesh.facets)
nvert = len(mesh.vertices)
print(f"nfaces={nface} nvert={nvert}")

# Thermal properties
prop = kalast.tpm.properties.DEIMOS
prop.se = STEFAN_BOLTZMANN * prop.emissivity
prop.compute_conductivity_diffusivity()
print(f"k={prop.conductivity:.6e} d={prop.diffusivity:.6e}")

# Time
date_start_pre = "2025-03-09 00:00"
date_start_simu = "2025-03-12 00:00"
date_stop = "2025-03-12 15:00"

et_start_pre = spice.str2et(date_start_pre)
et_start_sim = spice.str2et(date_start_simu)
et_stop = spice.str2et(date_stop)

dt_pre = 300  # 30 120 300
dt_sim = 120
t_tot = et_stop - et_start_pre
t_pre = et_start_sim - et_start_pre
t_sim = et_stop - et_start_sim
nit_pre = numpy.ceil(t_pre / dt_pre).astype(int) + 1
nit_sim = numpy.ceil(t_sim / dt_sim).astype(int) + 1
nit_tot = nit_pre + nit_sim
print(f"t_pre={t_pre}s ={t_pre / DAY:.3}d ({nit_pre}it)")
print(f"t_sim={t_sim}s ={t_sim / DAY:.3}d ({nit_sim}it)")

equator = pandas.read_csv("out/equator.csv")["index"].to_numpy()
meridian0 = pandas.read_csv("out/meridian0.csv")["index"].to_numpy()
equator_meridian0 = pandas.read_csv("out/equator_meridian0.csv")["index"].to_numpy()

# Interior, properties, initial temperatures.
dx0 = 2e-3
twodx0 = 2 * dx0
dx02 = dx0 * dx0
ls1 = kalast.tpm.properties.skin_depth_1(prop.diffusivity, deimos.spin_period)
ls2pi = kalast.tpm.properties.skin_depth_2pi(prop.diffusivity, deimos.spin_period)
ls2pi_orb = kalast.tpm.properties.skin_depth_2pi(
    prop.diffusivity, deimos.solar_orbit_period
)
maxdepth = ls2pi
z = numpy.arange(0, maxdepth + dx0, dx0, dtype=numpy.float32)
nx = z.size
nx_ls1 = (z <= ls1).sum()
nx_ls2pi = (z <= ls2pi).sum()
nx_save = (z <= 4 * ls1).sum()
dx = numpy.diff(z)
dx2in = dx[:-1] * dx[:-1]
dtpdx2in_pre = dt_pre / dx2in
dtpdx2in_sim = dt_sim / dx2in

print(
    f"dx={dx0:.4f} ls1={ls1:.4f}({nx_ls1}) ls2pi={ls2pi:.4f}({nx_ls2pi}) ls2pi_orb={ls2pi_orb:.4f} maxdepth={maxdepth:.4f}({nx})"
)

column = kalast.tpm.column.Column(z, prop, t_init=200.0)
columns = []
for ii in range(0, nface):
    columns.append(column.clone())

# Check convergence.
maxdt = kalast.tpm.core.stability_maxdt(prop.diffusivity, dx02)
S_pre = kalast.tpm.core.stability(prop.diffusivity, dt_pre, dx02)
S_sim = kalast.tpm.core.stability(prop.diffusivity, dt_sim, dx02)
print(f"max dt stable: {maxdt:.2f}")
print(
    f"Using dt={dt_pre}, stability={S_pre:.2f}, spin={deimos.spin_period / 3600:.3f}h({deimos.spin_period // dt_pre:.0f})"
)
print(
    f"Using dt={dt_sim}, stability={S_sim:.2f}, spin={deimos.spin_period / 3600:.3f}h({deimos.spin_period // dt_sim:.0f})"
)
if S_pre > 0.5:
    raise ValueError("Stability criteria not valid.")
if S_sim > 0.5:
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
ndigits = kalast.util.numdigits_comma(freqv)
digit = 10**ndigits

# Saving.
save_face = equator_meridian0[0]
et_sim = numpy.zeros(nit_sim)
sun_sim = numpy.zeros((nit_sim, 3))
tmp_surf_sim = numpy.zeros((nit_sim, nface))
tmp_cols_sim = numpy.zeros((nit_sim, nx))
print(f"Recording: {nit_sim}it (update: {progress_freq}%)")
print()

# Loop variables.
t = 0
it = 0
it_sim = 0
exporting = False
dtpdx2in = dtpdx2in_pre
dt = dt_pre

while True:
    et = et_start_pre + t

    (sun, _lt) = spice.spkpos("sun", et, deimos.frame, "none", deimos.name)
    sun *= 1e3
    sun = sun.astype(numpy.float32)

    # Prepare save data
    if not exporting and et >= et_start_sim:
        exporting = True
        dt = dt_sim
        dtpdx2in = dtpdx2in_sim
        print("Recording started.")

    if exporting:
        et_sim[it_sim] = et

    for ii in range(0, nface):
        p = mesh.facets[ii].pos
        n = mesh.facets[ii].normal

        v_sun = sun - p
        d_sun = numpy.linalg.norm(v_sun)
        dau_sun = d_sun / AU
        u_sun = v_sun / d_sun
        cosi = kalast.math.cosine_incidence(u_sun, n)

        # Get surface flux
        sflux = kalast.tpm.core.radiation_sun(dau_sun, cosi, prop.albedo)

        # Conduction of temperature
        columns[ii].t[0] = kalast.tpm.core.newton_method(
            columns[ii].t[0],
            sflux,
            prop.se,
            prop.conductivity,
            columns[ii].t[1],
            columns[ii].t[2],
            twodx0,
        )
        columns[ii].t[1:-1] = kalast.tpm.core.conduction_1d(
            columns[ii].t, columns[ii].d, dtpdx2in
        )
        columns[ii].t[-1] = columns[ii].t[-2]
        if columns[ii].t[0] is None:
            raise ValueError("Newton method never converged.")

    # Save data
    if exporting:
        tmp_surf_sim[it_sim] = numpy.array([column.t[0] for column in columns])
        tmp_cols_sim[it_sim] = columns[save_face].t
        sun_sim[it_sim] = sun

    # Show progress
    progress = it / (nit_tot - 1) * 100
    if ndigits > 0:
        progress = numpy.floor(progress * digit) / digit
    if progress >= last_freq_reached + freqv:
        last_freq_reached += freqv
        print(f"{progress:{digits_full}.{digits_decimal}f}% ({it:,}/{nit_tot - 1:,}it)")

    # Update loop
    if t >= t_tot:
        break

    t += dt
    it += 1

    if exporting:
        it_sim += 1

    if it == 1:
        timer_1 = time.perf_counter()


# Final show progress
if last_freq_reached < 100:
    print(f"{100:{digits_full}.{digits_decimal}f}% ({it:,}/{nit_tot - 1:,}it)")
print()
timer_2 = time.perf_counter()
timer_elapsed = timer_2 - timer_1
print(
    f"Simulation duration: {timer_elapsed:.4f}s ({numpy.floor((nit_tot - 1) / timer_elapsed):,}it/s)"
)
print(
    f"Avg surf temps: mean={tmp_surf_sim.mean():.2f} min={tmp_surf_sim.min():.2f} max={tmp_surf_sim.max():.2f}"
)
print(f"Record completed ({nit_sim}it)")

exit()

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
