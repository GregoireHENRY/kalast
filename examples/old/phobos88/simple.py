#!/usr/bin/env python

import copy
import math
import time
from pathlib import Path

import glm
import numpy
import trimesh
import spiceypy as spice
from matplotlib import pyplot
# import matplotlib

import kalast
from kalast.config import Config
from kalast.util import (
    AU,
    HOUR,
    DAY,
    # DPR,
    RPD,
    matpow,
    numdigits_comma,
    STEFAN_BOLTZMANN,
)
from kalast.astro import Body, matspin, matobliq, cosinc, Column
from kalast.tpm.core import (
    solar_radiation,
    newton_method,
    conduction_1d,
    stability_maxdt,
    stability,
    skin_depth_1,
    skin_depth_2pi,
    emittance,
)
from kalast.props import PROPERTIES

import plot


# General config.
cfg = Config()
cfg.run()


# Load spice
spice.furnsh("/Users/gregoireh/data/spice/all/mk/solar_system_v0054.tm")
spice.furnsh("/Users/gregoireh/data/spice/phobos88/spk/iam_r2.bsp")

# date_start = "1989-03-25 00:00"
date_start = "1989-03-21 00:00"
date_stop = "1989-03-26 00:00"
et_start = spice.str2et(date_start)
et_stop = spice.str2et(date_stop)

sc = kalast.spice_entities.phobos2
bod = kalast.spice_entities.phobos
frame = "j2000"

# Sun position in global reference frame.
# sun = glm.dvec3(1, 0, 0) * AU
# sun = glm.dvec3(0, 1, 0) * AU
(sun, _lt) = spice.spkpos("sun", et_start, frame, "none", bod.name)
sun *= 1e3
dau_sun_start = glm.length(sun) / AU

# Observer
# (p, _lt) = spice.spkpos(sc, et_start, frame, "none", bod.name)

# Body information.
body = Body()
spin_period = bod.spin_period
orbit_period = bod.orbit_period
solar_orbit_period = kalast.spice_entities.mars.orbital_period
body.spin = matspin(spin_period, glm.dvec3(0, 0, 1))
spin_init = matpow(body.spin, 0)
obl = matobliq(0 * RPD)
body.m = body.m * obl * spin_init

# Thermal properties.
prop = PROPERTIES["phobos"]
se = STEFAN_BOLTZMANN * prop.e
print(f"k={prop.k:.6e} d={prop.d:.6e}")

# Surface.
body.surf.mesh = trimesh.load("phobos.obj")
nface = body.surf.nfaces()
nvert = body.surf.nfaces()
body.surf.set_face_props_constant(prop)
sph = numpy.array([kalast.util.cart2sph(v) for v in body.surf.mesh.triangles_center])
print(f"nfaces={nface} nvert={nvert}")

# Interior, properties, initial temperatures.
c = Column()
dx0 = 2e-3
twodx0 = 2 * dx0
dx02 = dx0 * dx0
ls1 = skin_depth_1(prop.d, spin_period)
ls2pi = skin_depth_2pi(prop.d, spin_period)
ls2pi_orb = skin_depth_2pi(prop.d, orbit_period)
maxdepth = ls2pi
c.z = numpy.arange(0, maxdepth + dx0, dx0)
nx = c.z.size
nx_ls1 = (c.z <= ls1).sum()
nx_ls2pi = (c.z <= ls2pi).sum()
nx_save = (c.z <= 4 * ls1).sum()
# nx_save = nx
dx = numpy.diff(c.z)
dx2in = dx[:-1] * dx[:-1]
c.t = numpy.ones(nx) * 220.0
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
dt = 120  # 30 120 300
tf = 5 * DAY  # 5 20
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
t_save = 24 * HOUR
nii_save = t_save // dt
nii_hour = HOUR // dt
save_col = 1264
ts = numpy.zeros(nii_save)
tmp1 = numpy.zeros((nii_save, nface))
tmp2 = numpy.zeros((nii_save, nx))
fxe = numpy.zeros(nii_save)
illums = numpy.zeros((nii_save, nface, 3))
print(f"{nii_save} iterations will be recorded (frequence update: {progress_freq}%)")
print()

wl = 5.5e-6

# to update if dt changes.
m_spin = matpow(body.spin, dt)
dtpdx2in = dt / dx2in

# Loop variables.
t = 0
it = 0

while True:
    et = et_start + t

    (sun, _lt) = spice.spkpos("sun", et, bod.frame, "none", bod.name)
    sun *= 1e3
    (psc, _lt) = spice.spkpos(sc.name, et, bod.frame, "none", bod.name)
    psc *= 1e3

    # msp = spice.pxform(bod.frame, "j2000", et)
    # msp = glm.dmat4(msp)

    # Get body orientation and position wrt Sun
    # if it > 0:
    # body.m = body.m * m_spin
    # body.m = msp

    # m = body.ref * body.m
    # mn = glm.transpose(glm.inverse(glm.dmat3(m)))

    # Prepare save data
    if nit - it <= nii_save:
        ii_save = nii_save - nit + it
        ts[ii_save] = t

    # For all facets, get incidence angle and distance of Sun
    for ii in range(0, nface):
        p = body.surf.mesh.triangles_center[ii]
        n = body.surf.mesh.face_normals[ii]

        v_sun = sun - p
        d_sun = glm.length(v_sun)
        dau_sun = d_sun / AU
        u_sun = v_sun / d_sun
        cosi = cosinc(u_sun, n)

        v_sc = psc - p
        d_sc = glm.length(v_sc)
        u_sc = v_sc / d_sc
        cose = cosinc(u_sc, n)

        u_sun_proj = sun - glm.dot(u_sun, n) * n - p
        u_sun_proj = u_sun_proj / glm.length(u_sun_proj)
        u_sc_proj = psc - glm.dot(u_sc, n) * n - p
        u_sc_proj = u_sc_proj / glm.length(u_sc_proj)
        cos_pha_proj = cosinc(u_sun_proj, u_sc_proj)

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

        # Save data per facet
        if nit - it <= nii_save:
            illums[ii_save, ii, 0] = math.acos(cosi)
            illums[ii_save, ii, 1] = math.acos(cose)
            illums[ii_save, ii, 2] = math.acos(cos_pha_proj)

            fxe[ii_save] += emittance(
                body.inte[ii].t[0],
                wl,
                prop.e,
                body.surf.mesh.area_faces[ii] * 1e6,
                cose,
                d_sc,
            )

    # Save data
    if nit - it <= nii_save:
        tmp1[ii_save] = numpy.array([c.t[0] for c in body.inte])
        tmp2[ii_save] = body.inte[save_col].t

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
print(
    f"Avg surf temps: mean={tmp1.mean():.2f} min={tmp1.min():.2f} max={tmp1.max():.2f}"
)

# Prepare plot
ts -= ts[0]
ts /= HOUR
path_out = Path("out")
path_out.mkdir(parents=True, exist_ok=True)

numpy.save(path_out / "ts.npy", ts)
numpy.save(path_out / "z.npy", body.inte[save_col].z)

p = numpy.zeros(10)
p[0] = prop.a
p[1] = prop.e
p[2] = prop.ti
p[3] = prop.p
p[4] = dau_sun_start
p[5] = 0.9  # crater density
p[6] = 150  # crater opening angle
p[7] = 32  # ngrid points in crater wrt theta
p[8] = 32  # ngrid points in crater wrt phi
numpy.save(path_out / "roughness_input.npy", p)

full_tmp = numpy.zeros((len(body.inte), len(body.inte[0].t)))
for ii in range(len(body.inte)):
    full_tmp[ii] = body.inte[ii].t

numpy.save(path_out / "t_last.npy", full_tmp)
numpy.save(path_out / "t_surf.npy", tmp1)
numpy.save(path_out / "t_depth.npy", tmp2)
numpy.save(path_out / "illums.npy", illums)
numpy.save(path_out / "fxe.npy", fxe)

kalast.plot.style.load()
plot.depth(
    body.inte[save_col].z * 100,
    tmp2[: nii_save // 2 : nii_hour, :],
    path_out,
    (body.inte[save_col].z[-1] * 100, 0),
)
plot.daily_surf(
    ts,
    tmp2[:, 0],
    path_out,
    (0, t_save / HOUR),
    xlabel="Hours elapsed [h]",
    ylabel="Temperature [K]",
)
plot.daily_surf(
    ts,
    fxe,
    path_out / "thermal_emission.png",
    (0, t_save / HOUR),
    xlabel="Hours elapsed [h]",
    ylabel="Spectral irradiance [W/m3]",
)
plot.smap(body.surf, sph, tmp1[-1, :], 160, 300, path_out)
