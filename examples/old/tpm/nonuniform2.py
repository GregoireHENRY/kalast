#!/usr/bin/env python

# import matplotlib
import copy
import glm
import numpy
from matplotlib import pyplot
from pathlib import Path

import kalast  # noqa
from kalast.config import Config
from kalast.util import AU, HOUR, DAY, RPD, matpow, newton_method, numdigits_comma
from kalast.astro import Body, Vertex, matspin, matobliq, cosinc, Interior
from kalast.tpm import (
    # effective_temperature,
    sun_radiation,
    newton_method_fn_2,
    newton_method_dfn_2,
    conduction_1d_2,
    # conduction_one_layer,
    adiabatic,
    stability_maxdt,
    stability,
    skin_depth_1,
    skin_depth_2pi,
    # heat_capacity_tdep,
    conductivity_tdep,
    diffusivity,
    tridiag_implicit,
)
from kalast.props import PROPERTIES
from kalast import plot


cfg = Config()
cfg.run()

sun = glm.dvec3(1, 0, 0) * AU

body = Body()
spin_period = 12 * HOUR
orbit_period = 365 * 2 * HOUR
body.spin = matspin(spin_period, glm.dvec3(0, 0, 1))
spin_init = matpow(body.spin, 0)
obl = matobliq(0 * RPD)
body.m = body.m * obl * spin_init

prop = PROPERTIES["didymos"]

v = Vertex()
v.prop = copy.deepcopy(prop)
v.normal = glm.dvec3(1, 0, 0)
v.pos = glm.dvec3(100, 0, 0)
body.surf.append(v)
print(f"k={v.prop.k:.6e} d={v.prop.d:.6e}")

body.inte = Interior()
# body.inte.r350 = 0

dx0 = 1e-2
ls1 = skin_depth_1(v.prop.d, spin_period)
ls2pi = skin_depth_2pi(v.prop.d, spin_period)
ls1_orb = skin_depth_1(v.prop.d, orbit_period)
ls2pi_orb = skin_depth_2pi(v.prop.d, orbit_period)

# maxdepth = 8 * ls1
maxdepth = ls2pi
body.inte.z = numpy.arange(0, maxdepth + dx0, dx0)

# body.inte.z = kalast.tpm.spatial_grid(ls1)
# body.inte.z = kalast.tpm.spatial_grid(ls1, b=8)
# body.inte.z = kalast.tpm.spatial_grid(ls1, m=30, n=5, b=8)

# body.inte.z = numpy.array([(ls1 / 2) * 1.1**ii for ii in range(0, 20)])

nx = body.inte.z.size
nx_ls1 = (body.inte.z <= ls1).sum()
nx_ls2pi = (body.inte.z <= ls2pi).sum()
nx_save = (body.inte.z <= 4 * ls1).sum()
# nx_save = nx
print(
    f"dx={dx0:.4f} ls1={ls1:.4f}({nx_ls1}) ls2pi={ls2pi:.4f}({nx_ls2pi}) ls2pi_orb={ls2pi_orb:.4f} maxdepth={maxdepth:.4f}({nx})"
)

body.t = numpy.ones(nx) * 290

dx = numpy.diff(body.inte.z)
body.inte.p = numpy.ones(nx) * v.prop.p
body.inte.c = numpy.ones(nx) * v.prop.c

body.inte.k = numpy.ones(nx) * v.prop.k
# body.inte.k = conductivity_tdep(v.prop.k, body.t)
body.inte.d = diffusivity(body.inte.k, body.inte.p, body.inte.c)

d3x = dx[1:] * dx[:-1] * (dx[1:] + dx[:-1])
body.inte.g1 = 2 * dx[1:] / d3x[0:]
body.inte.g2 = 2 * dx[0:-1] / d3x[0:]

maxdt = stability_maxdt(dx, body.inte.d[:-1])
print(f"max dt stable: {maxdt:.2f}")

dt = 90
tf = 50 * DAY
t = 0
it = 0
nit = numpy.ceil((tf - t) / dt).astype(int) + 1
S = stability(body.inte.d[:-1], dt, dx)
print(f"Using dt={dt}, stability={S:.2f}")
print(f"simulation time={tf / DAY}days, {nit} it")

rcoef = dt / (body.inte.p * body.inte.c)

t_save = 24 * HOUR
nii_save = t_save // dt
nii_hour = HOUR // dt
time = numpy.zeros(nii_save)
stemp = numpy.zeros((nii_save, nx))
print(f"{nii_save} iterations will be recorded")
print()

last_time_it_r = 0

while True:
    # no spin @ first it
    if it:
        body.m = body.m * matpow(body.spin, dt)

    mr = body.ref * body.m
    mn = glm.transpose(glm.inverse(glm.dmat3(mr)))

    v = body.surf[0]
    p = mr * v.pos
    n = mn * v.normal
    # print(p, n)

    v_sun = sun - p
    u_sun = glm.normalize(v_sun)
    d_sun = glm.length(v_sun) / AU

    cosi = cosinc(u_sun, n)
    sflux = sun_radiation(d_sun, cosi, v.prop.a)
    # print(sflux)

    matrix = tridiag_implicit(nx, body.inte.k, dx, rcoef)

    # source = body.t + rcoef * sflux
    source = body.t.copy()

    s0, sN, b0, c0, aN, bN = set_flux_BC(rcoef, sflux)
    # source[0] = s0
    # source[-1] = sN
    # self.matrix[1, 0] = b0
    # self.matrix[1, -1] = bN
    # self.matrix[0, 1] = c0
    # self.matrix[2, -2] = aN

    args_fn = {
        "f": sflux,
        "e": v.prop.e,
        "k": v.prop.k,
        "subt": body.t[1:3],
        "r350": body.inte.r350,
        "dx": dx0,
    }
    args_dfn = {
        "e": v.prop.e,
        "k": v.prop.k,
        "subt": body.t[1:3],
        "r350": body.inte.r350,
        "dx": dx0,
    }

    body.inte.a = body.inte.g1 * body.inte.k[:-2]
    body.inte.b = body.inte.g2 * body.inte.k[1:-1]

    body.t[0] = newton_method(
        body.t[0], newton_method_fn_2, args_fn, newton_method_dfn_2, args_dfn
    )
    body.t[1:-1] = conduction_1d_2(
        body.t, body.inte.p, body.inte.c, body.inte.a, body.inte.b, dt
    )
    body.t[-1] = adiabatic(body.t[-2])

    # body.inte.c = heat_capacity_tdep()
    # body.inte.k = conductivity_tdep(v.prop.k, body.t, body.inte.r350)

    if nit - it <= nii_save:
        ii_save = nii_save - nit + it
        time[ii_save] = t
        stemp[ii_save] = body.t

    freq = 1
    r = it / nit * 100
    s = numdigits_comma(freq)
    if s > 0:
        digit = 10**s
        r = numpy.floor(r * digit) / digit
    if r >= last_time_it_r + freq:
        last_time_it_r = r
        print(f"{r:.{s}f}% ({it:,} / {nit:,} it)")

    # if it == 1:
    #     break

    if t >= tf:
        break
    t += dt
    it += 1

time -= time[0]
time /= HOUR

path_out = Path("out")
path_out.mkdir(parents=True, exist_ok=True)

plot.style.load()
fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Hours elapsed [h]")
ax.set_ylabel("Temperature [K]")
ax.plot(time, stemp[:, 0], lw=1, color="k")
ax.set_xlim(0, t_save / HOUR)
# ax.set_ylim(0, None)
# ax.set_yscale("log")
# pyplot.legend()
fig.savefig(path_out / "surf.png", bbox_inches="tight", dpi=300)
pyplot.show()

fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Temperature [K]")
ax.set_ylabel("Depth [cm]")
for ii in range(0, nii_save // 2, nii_hour):
    ax.plot(stemp[ii, :], body.inte.z * 100, lw=1, color="k")
# ax.set_xlim(0, None)
ax.set_ylim(0, body.inte.z[nx_save - 1] * 100)
ax.invert_yaxis()
fig.savefig(path_out / "depth_zoom.png", bbox_inches="tight", dpi=300)

ax.set_ylim(body.inte.z[-1] * 100, 0)
fig.savefig(path_out / "depth_full.png", bbox_inches="tight", dpi=300)
pyplot.show()
