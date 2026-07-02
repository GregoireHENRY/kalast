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

import kalast  # noqa
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
from kalast.spice_entities import didymos, dimorphos_pre as dimorphos  # noqa


cfg = Config()
cfg.run()

spice.kclear()
# spice.furnsh("/Users/gregoireh/data/spice/wgc/mk/solar_system_v0060.tm")
# spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops_local.tm")
# spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/mk/hera_plan_local.tm")
spice.furnsh("/Users/gregoireh/data/spice/dart/mk/d520_v03.tm")
frame = "j2000"

ents = [didymos, dimorphos]
bods = [Body() for _ in ents]
#     "ii_cols_save": [],
#     "nx": [],
#     "dx2in": [],
#     "dtpdx2in": [],
#     "dtpdx2in_save": [],

solar_orbit_period = didymos.orbit_period

date_start = "2018-08-01 00:00"
date_start_export_avg = "2020-08-01 00:00"
date_start_export_detail = "2022-08-23 00:00"
date_stop = "2022-08-24 12:00"

et_start = spice.str2et(date_start)
et_start_export_avg = spice.str2et(date_start_export_avg)
et_start_export_detail = spice.str2et(date_start_export_detail)
et_stop = spice.str2et(date_stop)

# Time.
dt = 120  # 30 120 300
dt_save = 30
tf = et_stop - et_start
t_prep_avg = et_start_export_avg - et_start
t_prep_detail = et_start_export_detail - et_start
t_save_avg = et_stop - et_start_export_avg
t_save_detail = et_stop - et_start_export_detail
nit_prep_avg = numpy.ceil(t_prep_avg / dt).astype(int) + 1
nit_prep_detail = numpy.ceil(t_prep_detail / dt).astype(int) + 1
nit_save_avg = numpy.ceil(t_save_avg / dt_save).astype(int) + 1
nit_save_detail = numpy.ceil(t_save_detail / dt_save).astype(int) + 1
nit = nit_prep_detail + nit_save_detail
nit_hour = HOUR // dt
nit_day = DAY // dt
print(f"simulation time: {tf / DAY:.3}days ({nit}it), dt={dt}s")
print(
    f"at {nit_prep_avg / nit * 100:.0f}% of the simulation, dt={dt_save}s (for {nit - nit_prep_avg}it)"
)

# Surface.
for iib, (ent, bod) in enumerate(zip(ents, bods)):
    bod.name = ent.name.lower()
    print(f"{bod.name} ({iib})")

    # Thermal properties.
    prop = PROPERTIES[bod.name]
    print(f"  k={prop.k:.6e} d={prop.d:.6e}")

    bod.surf.mesh = trimesh.load(f"work/{bod.name}/mesh.obj")
    nface = bod.surf.nfaces()
    nvert = bod.surf.nvertices()
    bod.surf.set_props_constant_face(prop)
    print(f"  nfaces={nface} nvert={nvert}")

    equator = numpy.load(f"work/{bod.name}/equator.npy")
    meridian0 = numpy.load(f"work/{bod.name}/meridian0.npy")
    equator_meridian0 = numpy.load(f"work/{bod.name}/equator_meridian0.npy")

    bod.params = {}
    bod.params["ii_cols_save"] = equator_meridian0[0]

    # Interior, properties, initial temperatures.
    c = Column()
    dx0 = 4e-3
    twodx0 = 2 * dx0
    dx02 = dx0 * dx0
    ls1 = skin_depth_1(prop.d, ent.spin_period)
    ls2pi = skin_depth_2pi(prop.d, ent.spin_period)
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
    bod.params["nx"] = nx
    bod.params["dx2in"] = dx2in
    bod.params["dtpdx2in"] = dtpdx2in
    bod.params["dtpdx2in_save"] = dtpdx2in_save

    c.set_temp_constant(100.0)
    c.set_props_constant(prop)
    print(
        f"  dx={dx0:.4f} ls1={ls1:.4f}({nx_ls1}) ls2pi={ls2pi:.4f}({nx_ls2pi}) ls2pi_orb={ls2pi_orb:.4f} maxdepth={maxdepth:.4f}({nx})"
    )
    for _ in range(0, nface):
        bod.inte.append(copy.deepcopy(c))

    # Check convergence.
    maxdt = stability_maxdt(dx02, prop.d)
    print(f"  max dt stable: {maxdt:.2f}")
    S = stability(prop.d, dt, dx02)
    print(
        f"  stability={S:.2f}, spin={ent.spin_period / 3600:.3f}h({ent.spin_period // dt:.0f})"
    )
    if S > 0.5:
        raise ValueError("Stability criteria not valid.")

    bod.save = {}
    bod.save["sun"] = numpy.zeros((nit_save_detail, 3))
    bod.save["tmp_surf"] = numpy.zeros((nit_save_detail, nface))
    bod.save["tmp_cols"] = numpy.zeros((nit_save_detail, nx))
    bod.save["tmp_surf_max"] = numpy.zeros(nface)
    bod.save["tmp_surf_avg"] = numpy.zeros(nface)
    bod.save["tmp_surf_min"] = numpy.ones(nface) * 400


# Time loop progress.
progress_freq = "0.5"
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
et_save = numpy.zeros(nit_save_detail)
print(f"Recording: {nit_save_detail}it (update: {progress_freq}%)")
print()

# Loop variables.
t = 0
it = 0
it_save_avg = 0
it_save_detail = 0
exporting_detail = False
exporting_avg = False

while True:
    et = et_start + t

    # Prepare save data
    if not exporting_avg and et >= et_start_export_avg:
        exporting_avg = True
        dt = dt_save
        for bod in bods:
            bod.params["dtpdx2in"] = bod.params["dtpdx2in_save"]
        print("Recording avg started.")

    if not exporting_detail and et >= et_start_export_detail:
        exporting_detail = True
        print("Recording detail started.")

    if exporting_detail:
        et_save[it_save_detail] = et

    for iib, bod in enumerate(bods):
        (sun, _lt) = spice.spkpos("sun", et, ents[iib].frame, "none", ents[iib].name)
        sun *= 1e3

        for ii in range(0, nface):
            p = bod.surf.mesh.triangles_center[ii] * 1e3
            n = bod.surf.mesh.face_normals[ii]

            v_sun = sun - p
            d_sun = glm.length(v_sun)
            dau_sun = d_sun / AU
            u_sun = v_sun / d_sun
            cosi = cosinc(u_sun, n)

            # Get surface flux
            sflux = solar_radiation(dau_sun, cosi, bod.surf.a[ii])

            # Conduction of temperature
            bod.inte[ii].t[0] = newton_method(
                bod.inte[ii].t[0],
                sflux,
                STEFAN_BOLTZMANN * bod.surf.e[ii],
                bod.inte[ii].k[0],
                bod.inte[ii].t[1:3],
                twodx0,
            )

            a = conduction_1d(bod.inte[ii].t, bod.inte[ii].d, bod.params["dtpdx2in"])
            bod.inte[ii].t[1:-1] = a
            bod.inte[ii].t[-1] = bod.inte[ii].t[-2]
            if bod.inte[ii].t[0] is None:
                raise ValueError("Newton method never converged.")

        # Save data
        if exporting_avg:
            tmp_surf = numpy.array([c.t[0] for c in bod.inte])
            bod.save["tmp_surf_max"] = numpy.max(
                (bod.save["tmp_surf_max"], tmp_surf), axis=0
            )
            bod.save["tmp_surf_avg"] = (
                bod.save["tmp_surf_avg"] * it_save_avg + tmp_surf
            ) / (it_save_avg + 1)
            bod.save["tmp_surf_min"] = numpy.min(
                (bod.save["tmp_surf_min"], tmp_surf), axis=0
            )

        if exporting_detail:
            bod.save["sun"][it_save_detail] = sun
            bod.save["tmp_surf"][it_save_detail] = numpy.array(
                [c.t[0] for c in bod.inte]
            )
            bod.save["tmp_cols"][it_save_detail] = bod.inte[
                bod.params["ii_cols_save"]
            ].t

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

    if exporting_avg:
        it_save_avg += 1

    if exporting_detail:
        it_save_detail += 1

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
for iib, bod in enumerate(bods):
    print(
        f"Avg surf temps {iib}: mean={bod.save['tmp_surf'].mean():.2f} min={bod.save['tmp_surf'].min():.2f} max={bod.save['tmp_surf'].max():.2f}"
    )
print(f"Record detail completed ({nit_save_detail}it)")

nbytes = 0

n_saved = it_save_detail + 1
et = et_save[:n_saved]
nbytes += et.nbytes
numpy.save("et_simu.npy", et)

for iib, bod in enumerate(bods):
    Path(bod.name).mkdir()
    bod.save["sun"] = bod.save["sun"][:, :n_saved]
    bod.save["tmp_surf"] = bod.save["tmp_surf"][:, :n_saved]
    bod.save["tmp_cols"] = bod.save["tmp_cols"][:, :n_saved]

    z = bod.inte[bod.params["ii_cols_save"]].z
    nbytes += z.nbytes
    nbytes += bod.save["sun"].nbytes
    nbytes += bod.save["tmp_surf"].nbytes
    nbytes += bod.save["tmp_cols"].nbytes

    numpy.save(f"{bod.name}/z.npy", z)
    numpy.save(f"{bod.name}/sun.npy", bod.save["sun"])
    numpy.save(f"{bod.name}/tmp_surf.npy", bod.save["tmp_surf"])
    numpy.save(f"{bod.name}/tmp_cols.npy", bod.save["tmp_cols"])

    numpy.save(f"{bod.name}/tmp_surf_max.npy", bod.save["tmp_surf_max"])
    numpy.save(f"{bod.name}/tmp_surf_avg.npy", bod.save["tmp_surf_avg"])
    numpy.save(f"{bod.name}/tmp_surf_min.npy", bod.save["tmp_surf_min"])

    tmp_state_save = numpy.zeros((len(bod.inte), len(bod.inte[0].t)))
    for ii in range(len(bod.inte)):
        tmp_state_save[ii] = bod.inte[ii].t

    nbytes += tmp_state_save.nbytes
    numpy.save(f"{bod.name}/tmp_state.npy", tmp_state_save)

print(f"Exported {nbytes / 1e6:.3f}MB of data")
