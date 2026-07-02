#!/usr/bin/env python

import time
from pathlib import Path

import numpy
import pandas
import scipy
from matplotlib import pyplot

import kalast
from kalast.util import AU, HOUR, DAY


# Config
# ------

sun = numpy.array([1.0, 0.0, 0.0]) * AU
temperature_init = 290.0

# surface position and normal
p0 = numpy.array([100.0, 0.0, 0.0])
n0 = numpy.array([1.0, 0.0, 0.0])

spin_period = 6.0 * HOUR
spin_axis = numpy.array([0.0, 0.0, 1.0])

albedo = 0.1
emissivity = 0.9
density = 2000.0
heat_capacity = 600.0
thermal_inertia = 400.0

delta_depth = 1e-2

delta_time = 300.0
duration_total = 20.0 * DAY
duration_save = 12 * HOUR

progress_freq = "10"
digits_full = 3
digits_decimal = 0


# Prepare simulation
# ------------------

se = kalast.util.STEFAN_BOLTZMANN * emissivity
conductivity = kalast.tpm.properties.conductivity(
    thermal_inertia, density, heat_capacity
)
diffusivity = kalast.tpm.properties.diffusivity(conductivity, density, heat_capacity)
ls1 = kalast.tpm.properties.skin_depth_1(diffusivity, spin_period)
ls2pi = kalast.tpm.properties.skin_depth_2pi(diffusivity, spin_period)

twodx0 = 2 * delta_depth
dx02 = delta_depth * delta_depth
x = numpy.arange(0, ls2pi + delta_depth, delta_depth)
dx = numpy.diff(x)
dx2in = dx[:-1] * dx[:-1]

nx_ls1 = (x <= ls1).sum()
nx_ls2pi = (x <= ls2pi).sum()
nx_save = (x <= 3 * ls1).sum()

print(f"k={conductivity:e} d={diffusivity:e}")
print(f"ls1={ls1:.5f} ls2={ls2pi:.5f}")
print(f"xmax={x.max()} dx={delta_depth} nx={x.size}")
print()

dtpdx2in = delta_time / dx2in
darr = numpy.ones(x.size) * diffusivity
tmp = numpy.ones(x.size) * temperature_init

nit = numpy.ceil(duration_total / delta_time).astype(int) + 1
S = kalast.tpm.core.stability(diffusivity, delta_time, dx02)
maxdt = kalast.tpm.core.stability_maxdt(diffusivity, dx02, 0.5)

print(f"duration_total={duration_total / DAY}d")
print(f"dt={delta_time} S={S:.2f} nit={nit}")
print(f"max_dt_stable={maxdt:.2f}")
if S > 0.5:
    raise ValueError("Stability criteria not valid.")
print()

digits = [len(_d) for _d in progress_freq.split(".")]
if len(digits) == 2:
    digits_decimal = digits[1]
    if digits_decimal > 0:
        digits_full += digits_decimal + 1
freqv = float(progress_freq)
last_freq_reached = -freqv
ndigits = kalast.util.numdigits_comma(freqv)
digit = 10**ndigits

nii_save = int(numpy.floor(duration_save / delta_time))
nii_hour = int(numpy.floor(HOUR / delta_time))
ts = numpy.zeros(nii_save)
tmp_save = numpy.zeros((nii_save, x.size))

print(f"duration_record={duration_save / HOUR}h")
print(f"nit_record={nii_save} (freq_update={progress_freq}%)")
print()

m_spin = kalast.util.mat_axis_angle(
    spin_axis, 2.0 * numpy.pi * delta_time / spin_period
)
m_state = numpy.eye(3)


# Simulation
# ----------

t = 0
it = 0
while True:
    if it > 0:
        m_state = m_spin @ m_state

    p = m_state @ p0
    n = m_state @ n0
    v_sun = sun - p
    d_sun = numpy.linalg.norm(v_sun)
    dau_sun = d_sun / AU
    u_sun = v_sun / d_sun
    cosi = kalast.math.cosine_incidence(u_sun, n)
    sflux = kalast.tpm.core.radiation_sun(dau_sun, cosi, albedo)

    tmp = kalast.tpm.routine.update_thermal_state(
        tmp, sflux, darr, dtpdx2in, se, conductivity, twodx0
    )

    if nit - it <= nii_save:
        ii_save = nii_save - nit + it
        ts[ii_save] = t
        tmp_save[ii_save] = tmp.copy()

    progress = it / (nit - 1) * 100
    if ndigits > 0:
        progress = numpy.floor(progress * digit) / digit
    if progress >= last_freq_reached + freqv:
        last_freq_reached += freqv
        print(f"{progress:{digits_full}.{digits_decimal}f}% ({it:,}/{nit - 1:,}it)")

    if t >= duration_total:
        break
    t += delta_time
    it += 1
    if it == 1:
        timer_1 = time.perf_counter()

if last_freq_reached < 100:
    print(f"{100:{digits_full}.{digits_decimal}f}% ({it:,}/{nit - 1:,}it)")
print()
timer_2 = time.perf_counter()
timer_elapsed = timer_2 - timer_1
print(
    f"Simulation duration = {timer_elapsed:.4f}s ({numpy.floor((nit - 1) / timer_elapsed):,}it/s)"
)


# Plot and export
# ---------------

tmp = tmp_save
ts -= ts[0]
ts /= HOUR
out = Path("out")
out.mkdir(parents=True, exist_ok=True)

# surface

df = {}
df["t"] = ts
df["tpm"] = tmp[:, 0]
df = pandas.DataFrame(df)
df.to_csv(out / "tmp_surf.csv", index=False, encoding="utf-8-sig")

kalast.plot.style.load()
fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Hours elapsed [h]")
ax.set_ylabel("Temperature [K]")

ax.plot(ts, tmp[:, 0], lw=1, color="k")

# tnew = numpy.arange(ts[0], ts[-1] + 0.001, 0.001)
# tmp_v_time = scipy.interpolate.make_smoothing_spline(ts, tmp[:, 0], lam=0.0)
# ax.plot(tnew, tmp_v_time(tnew), lw=1, color="r")

ax.set_xlim(0, duration_save / HOUR)
ax.set_ylim(240, 360)
# ax.set_yscale("log")
# pyplot.legend()
fig.savefig(out / "tmp_surf.svg", bbox_inches="tight")

# depth

df = {}
df["x[cm]"] = x * 100.0

xnew = numpy.arange(x[0], x[-1] + 0.001, 0.001) * 100.0

fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Temperature [K]")
ax.set_ylabel("Depth [cm]")
for ii in range(0, nii_save // 2, nii_hour):
    df[f"tpm:{ii}"] = tmp[ii, :]
    # ax.plot(tmp[ii, :], x * 100.0, lw=1, color="k")

    tmp_v_depth = scipy.interpolate.make_smoothing_spline(
        x * 100.0, tmp[ii, :], lam=0.0
    )
    ax.plot(tmp_v_depth(xnew), xnew, lw=1, color="k", ls="-")
ax.set_xlim(240, 360)

# 4 * ls1
ax.set_ylim(0, x[nx_save - 1] * 100.0)
ax.invert_yaxis()
fig.savefig(out / "tpm_depth_zoom.svg", bbox_inches="tight")

# ls2pi
ax.set_ylim(x[-1] * 100, 0)
fig.savefig(out / "tpm_depth.svg", bbox_inches="tight")
pyplot.show()

df = pandas.DataFrame(df)
df.to_csv(out / "tmp_depth.csv", index=False, encoding="utf-8-sig")
