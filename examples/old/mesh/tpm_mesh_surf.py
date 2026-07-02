#!/usr/bin/env python

import copy
import math
import time
from pathlib import Path

import glm
import numpy
import trimesh
from matplotlib import pyplot
import matplotlib

import kalast
from kalast.config import Config
from kalast.util import (
    AU,
    HOUR,
    DAY,
    DPR,
    RPD,
    matpow,
    numdigits_comma,
    STEFAN_BOLTZMANN,
)
from kalast.astro import Body, matspin, matobliq, cosinc, Column
from kalast.tpm.core import (
    sun_radiation,
    newton_method,
    conduction_1d,
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
sun = glm.dvec3(1, 0, 0) * AU

# Body information.
body = Body()
spin_period = 12 * HOUR
orbit_period = 365 * 2 * HOUR
body.spin = matspin(spin_period, glm.dvec3(0, 0, 1))
spin_init = matpow(body.spin, 0)
obl = matobliq(0 * RPD)
body.m = body.m * obl * spin_init

# Thermal properties.
prop = PROPERTIES["didymos"]
se = STEFAN_BOLTZMANN * prop.e
print(f"k={prop.k:.6e} d={prop.d:.6e}")

# Surface.
body.surf.mesh = trimesh.load("../ico.obj")
nface = body.surf.nfaces()
nvert = body.surf.nfaces()
body.surf.set_face_props_constant(prop)
sph = numpy.array([kalast.util.cart2sph(v) for v in body.surf.mesh.triangles_center])
print(f"nfaces={nface} nvert={nvert}")

# Interior, properties, initial temperatures.
c = Column()
dx0 = 1e-2
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
c.t = numpy.ones(nx) * 290.0
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
save_col = 0
ts = numpy.zeros(nii_save)
tmp1 = numpy.zeros((nii_save, nface))
tmp2 = numpy.zeros((nii_save, nx))
print(f"{nii_save} iterations will be recorded (frequence update: {progress_freq}%)")
print()

# to update if dt changes.
m_spin = matpow(body.spin, dt)
dtpdx2in = dt / dx2in

# Loop variables.
t = 0
it = 0

while True:
    # Get body orientation and position wrt Sun
    if it > 0:
        body.m = body.m * m_spin
    m = body.ref * body.m
    mn = glm.transpose(glm.inverse(glm.dmat3(m)))

    # For all facets, get incidence angle and distance of Sun
    for ii in range(0, nface):
        p = m * body.surf.mesh.triangles_center[ii]
        n = mn * body.surf.mesh.face_normals[ii]

        v_sun = sun - p
        d_sun = glm.length(v_sun)
        dau_sun = d_sun / AU
        u_sun = v_sun / d_sun
        cosi = cosinc(u_sun, n)

        # Get surface flux
        sflux = sun_radiation(dau_sun, cosi, body.surf.a[ii])

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
    if nit - it <= nii_save:
        ii_save = nii_save - nit + it
        ts[ii_save] = t
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

# Prepare plot
ts -= ts[0]
ts /= HOUR
path_out = Path("out")
path_out.mkdir(parents=True, exist_ok=True)

kalast.plot.style.load()
fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Hours elapsed [h]")
ax.set_ylabel("Temperature [K]")
ax.plot(ts, tmp2[:, 0], lw=1, color="k")
ax.set_xlim(0, t_save / HOUR)
# ax.set_ylim(0, None)
# ax.set_yscale("log")
# pyplot.legend()
fig.savefig(path_out / "surf.png", bbox_inches="tight", dpi=300)
# pyplot.show()

fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Temperature [K]")
ax.set_ylabel("Depth [cm]")
for ii in range(0, nii_save // 2, nii_hour):
    ax.plot(tmp2[ii, :], body.inte[save_col].z * 100, lw=1, color="k")
# ax.set_xlim(0, None)
ax.set_ylim(0, body.inte[save_col].z[nx_save - 1] * 100)
ax.invert_yaxis()
fig.savefig(path_out / "depth_zoom.png", bbox_inches="tight", dpi=300)

ax.set_ylim(body.inte[save_col].z[-1] * 100, 0)
fig.savefig(path_out / "depth_full.png", bbox_inches="tight", dpi=300)

fig, axs = pyplot.subplots(2, 1, figsize=(15, 7.3), height_ratios=[9.5, 0.5])
ax = axs[0]
ax.set_xlabel("Longitude (°)")
ax.set_ylabel("Latitude (°)")
ax.set_xlim(-180, 180)
ax.set_ylim(-90, 90)
loc = matplotlib.ticker.MultipleLocator(base=30)
ax.xaxis.set_major_locator(loc)
loc = matplotlib.ticker.MultipleLocator(base=30)
ax.yaxis.set_major_locator(loc)
cnorm = matplotlib.colors.Normalize(vmin=220, vmax=360)
cmap = matplotlib.cm.cividis.resampled(14)
mappable = matplotlib.cm.ScalarMappable(cmap=cmap, norm=cnorm)
lon = sph[:, 0]
lat = sph[:, 1]
cnormv = cnorm(tmp1[0])
cmapv = cmap(cnormv)

for jj in range(0, sph.shape[0]):
    a = kalast.util.cart2sph(body.surf.mesh.triangles[jj, 0])[:2] * DPR
    b = kalast.util.cart2sph(body.surf.mesh.triangles[jj, 1])[:2] * DPR
    c = kalast.util.cart2sph(body.surf.mesh.triangles[jj, 2])[:2] * DPR
    trisph = numpy.array([a, b, c])
    trisph2 = None
    s1 = b - a
    s2 = c - b
    s3 = a - c
    d1 = numpy.linalg.norm(s1)
    d2 = numpy.linalg.norm(s2)
    d3 = numpy.linalg.norm(s3)
    cond = numpy.array([d1, d2, d3]) > 200
    condx = numpy.abs(numpy.array([s1[0], s2[0], s3[0]])) > 180
    condy = numpy.abs(numpy.array([s1[1], s2[1], s3[1]])) > 180
    if condx.sum() >= 1:
        # print(jj, a, b, c, s1, s2, s3, condx)
        signp = []
        signn = []
        for kk in range(0, 3):
            if trisph[kk, 0] >= 0:
                signp.append(kk)
            else:
                signn.append(kk)
        # print(signp, signn)
        for kk in signn:
            trisph[kk, 0] = 360 - abs(trisph[kk, 0])
        trisph2 = numpy.array([a, b, c])
        for kk in signp:
            if trisph[kk, 0] != 0:
                trisph2[kk, 0] = -360 + abs(trisph[kk, 0])
        # print(trisph[:, 0])
        # print(trisph2[:, 0])
    ax.fill(
        trisph[:, 0],
        trisph[:, 1],
        color=cmapv[jj],
        edgecolor="k",
        lw=1,
        joinstyle="bevel",
    )
    if trisph2 is not None:
        ax.fill(
            trisph2[:, 0],
            trisph2[:, 1],
            color=cmapv[jj],
            edgecolor="k",
            lw=1,
            joinstyle="bevel",
        )
        trisph2 = None

ax = axs[1]
ax.set_visible(False)
cax = fig.add_axes([0.26, 0.04, 0.5, 0.03])
_cb = fig.colorbar(
    mappable,
    label="Temperature (K)",
    orientation="horizontal",
    cax=cax,
    # ticks=params.cbar.get_ticks(),
    # format=params.cbar.get_formatter(),
)

fig.savefig(path_out / "tmap.png", bbox_inches="tight", dpi=300)
