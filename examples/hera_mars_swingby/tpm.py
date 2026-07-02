#!/usr/bin/env python

import time
from pathlib import Path  # noqa

import matplotlib
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
kernel_file = "/Users/gregoireh/data/spice/hera/kernels/mk/hera_ops_local.tm"
spice.furnsh(kernel_file)
# spice.furnsh("/Users/gregoireh/data/spice/wgc/mk/solar_system_v0060.tm")
frame = "j2000"

# Body
deimos = kalast.entity.DEIMOS
deimos.solar_orbit_period = kalast.entity.MARS.orbit_period

# Surface

mesh_file = "/Users/gregoireh/data/spice/hera/kernels/dsk/deimos_k005_tho_v02.obj"
mesh = kalast.mesh.Mesh(mesh_file)
mesh.flatten()
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
date_start_sim = "2025-03-12 00:00"
date_stop = "2025-03-12 15:00"

et_start_pre = spice.str2et(date_start_pre)
et_start_sim = spice.str2et(date_start_sim)
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

sph = pandas.read_csv("out/sph.csv").to_numpy()
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
nx_rec = (z <= 4 * ls1).sum()
dx = numpy.diff(z)
dx2in = dx[:-1] * dx[:-1]
dtpdx2in_pre = dt_pre / dx2in
dtpdx2in_sim = dt_sim / dx2in

print(
    f"dx={dx0:.4f} ls1={ls1:.4f}({nx_ls1}) ls2pi={ls2pi:.4f}({nx_ls2pi}) ls2pi_orb={ls2pi_orb:.4f} maxdepth={maxdepth:.4f}({nx})"
)

t_init = 200.0
column = kalast.tpm.column.Column(z, prop, t_init)
columns = []
for ii in range(0, nface):
    columns.append(column.clone())

# Check convergence.
maxdt_stable = kalast.tpm.core.stability_maxdt(prop.diffusivity, dx02)
S_pre = kalast.tpm.core.stability(prop.diffusivity, dt_pre, dx02)
S_sim = kalast.tpm.core.stability(prop.diffusivity, dt_sim, dx02)
print(f"max dt stable: {maxdt_stable:.2f}")
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
face_rec = equator_meridian0[0]
ets_sim = numpy.zeros(nit_sim)
sun_sim = numpy.zeros((nit_sim, 3))
tmp_surf_sim = numpy.zeros((nit_sim, nface))
tmp_cols_sim = numpy.zeros((nit_sim, nx))
print(f"Recording: {nit_sim}it (update: {progress_freq}%)")
print()

# Loop variables.
t = 0
it = 0
it_rec = 0
exporting = False
dtpdx2in = dtpdx2in_pre
dt = dt_pre

while True:
    et = et_start_pre + t

    (sun, _lt) = spice.spkpos("sun", et, deimos.frame, "none", deimos.name)
    sun *= 1e3
    sun = sun.astype(numpy.float32)

    # Prepare recording data
    if not exporting and et >= et_start_sim:
        exporting = True
        dt = dt_sim
        dtpdx2in = dtpdx2in_sim
        print("Recording started.")

    if exporting:
        ets_sim[it_rec] = et

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
        tmp_surf_sim[it_rec] = numpy.array([column.t[0] for column in columns])
        tmp_cols_sim[it_rec] = columns[face_rec].t
        sun_sim[it_rec] = sun

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
        it_rec += 1

    if it == 1:
        timer_1 = time.perf_counter()


# Final show progress
n_saved = it_rec + 1
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
print(f"Record completed ({n_saved}it)")


ets = ets_sim[:n_saved]
sun = sun_sim[:n_saved]
tmp_surf = tmp_surf_sim[:n_saved]
tmp_cols = tmp_cols_sim[:n_saved]

z = columns[face_rec].z

tmp_state = numpy.zeros((nface, nx))
for ii in range(nface):
    tmp_state[ii] = columns[ii].t

nbytes = 0
nbytes += ets.nbytes
nbytes += z.nbytes
nbytes += sun.nbytes
nbytes += tmp_surf.nbytes
nbytes += tmp_cols.nbytes
nbytes += tmp_state.nbytes
print(f"Exporting {nbytes / 1e6:.3f}MB of data")

df = {}
df["kernel"] = kernel_file
df["body"] = deimos.name
df["spin_period"] = deimos.spin_period
df["solar_orbit_period"] = deimos.solar_orbit_period
df["mesh"] = mesh_file
df["nface"] = nface
df["nvert"] = nvert
df["albedo"] = prop.albedo
df["emissivity"] = prop.emissivity
df["conductivity"] = prop.conductivity
df["diffusivity"] = prop.diffusivity
df["thermal_inertia"] = prop.thermal_inertia
df["date_start_pre"] = date_start_pre
df["date_start_sim"] = date_start_sim
df["date_stop"] = date_stop
df["et_start_pre"] = et_start_pre
df["et_start_sim"] = et_start_sim
df["et_stop"] = et_stop
df["dt_pre"] = dt_pre
df["dt_sim"] = dt_sim
df["nit_sim"] = nit_sim
df["dx0"] = dx0
df["ls1"] = ls1
df["ls2pi"] = ls2pi
df["ls2pi_orb"] = ls2pi_orb
df["maxdepth"] = maxdepth
df["nx"] = nx
df["t_init"] = t_init
df["maxdt_stable"] = maxdt_stable
df["S_pre"] = S_pre
df["S_sim"] = S_sim
df = pandas.DataFrame(df, index=[0])
df.to_csv("out/settings.csv", index=False, encoding="utf-8-sig")

df = {}
df["time"] = ets
df = pandas.DataFrame(df)
df.to_csv("out/ets_sim.csv", index=False, encoding="utf-8-sig")

df = {}
df["depth"] = z
df = pandas.DataFrame(df)
df.to_csv("out/z.csv", index=False, encoding="utf-8-sig")

df = {}
for iif in range(nface):
    df[iif] = tmp_surf[:, iif]
df = pandas.DataFrame(df)
df.to_csv("out/tmp_surf.csv", index=False, encoding="utf-8-sig")

df = {}
for iiz in range(nx):
    df[iiz] = tmp_cols[:, iiz]
df = pandas.DataFrame(df)
df.to_csv("out/tmp_cols.csv", index=False, encoding="utf-8-sig")

df = {}
for iiz in range(nx):
    df[iiz] = tmp_state[:, iiz]
df = pandas.DataFrame(df)
df.to_csv("out/tmp_state.csv", index=False, encoding="utf-8-sig")

df = {}
df["sun_x"] = sun[:, 0]
df["sun_y"] = sun[:, 1]
df["sun_z"] = sun[:, 2]
df[f"{deimos.name}_x"] = numpy.zeros(n_saved)
df[f"{deimos.name}_y"] = numpy.zeros(n_saved)
df[f"{deimos.name}_z"] = numpy.zeros(n_saved)
df[f"{deimos.name}_m00"] = numpy.ones(n_saved)
df[f"{deimos.name}_m01"] = numpy.zeros(n_saved)
df[f"{deimos.name}_m02"] = numpy.zeros(n_saved)
df[f"{deimos.name}_m10"] = numpy.zeros(n_saved)
df[f"{deimos.name}_m11"] = numpy.ones(n_saved)
df[f"{deimos.name}_m12"] = numpy.zeros(n_saved)
df[f"{deimos.name}_m20"] = numpy.zeros(n_saved)
df[f"{deimos.name}_m21"] = numpy.zeros(n_saved)
df[f"{deimos.name}_m22"] = numpy.ones(n_saved)
df = pandas.DataFrame(df)
df.to_csv("out/state.csv", index=False, encoding="utf-8-sig")

kalast.plot.style.load()
kalast.plot.util.depth(
    z * 100, tmp_cols[:, :], ylim=(z[-1] * 100, 0), name="out/depth.png"
)
kalast.plot.util.daily_surf(
    ets,
    tmp_cols[:, 0],
    # xlim=(0, None),
    xlabel="Ephemeris time",
    ylabel="Temperature [K]",
    name="out/surf.png",
)


mappable = matplotlib.cm.ScalarMappable(
    cmap=matplotlib.cm.inferno.resampled(100), norm=None
)
colors = mappable.to_rgba(tmp_surf[-1, :])
kalast.plot.util.smap(
    mesh, colors, label="temperature [K]", mappable=mappable, name="out/smap.png"
)

# cnorm = matplotlib.colors.Normalize(vmin=stmp.min(), vmax=stmp.max())
# cmap = matplotlib.cm.inferno
# mappable = matplotlib.cm.ScalarMappable(cmap=cmap, norm=cnorm)
# cnormv = cnorm(stmp)
# cmapv = cmap(cnormv)
# mesh.vertices = mesh.vertices * 1e-3
# mesh.unmerge_vertices()
# mesh.visual.face_colors = cmapv
# mesh.show()