#!/usr/bin/env python

import time
from pathlib import Path  # noqa

import symfit
import numpy
import pandas
import spiceypy as spice
from matplotlib import pyplot
# from joblib import Parallel, delayed

import kalast

# import setup
# import read_obs_new as read_obs


V0_NM = kalast.util.BAND_V0 * 1e-9


def apparent_reduced_vmag(flux: float, dau_sun_x_obs: float) -> float:
    return -2.5 * numpy.log10(flux / V0_NM) - 5.0 * numpy.log10(dau_sun_x_obs)


# if Path(p).name.split("_")[0] == "dimorphos":

nfs = []
meshes = []

# "/Users/gregoireh/data/mesh/didymos/didymos_g_9309mm_spc_obj_0000n00000_v003_decimated_3072.obj",
# "/Users/gregoireh/data/mesh/dimorphos/dimorphos_g_1940mm_spc_obj_0000n00000_v004_decimated_3072.obj",
paths = ["/Users/gregoireh/data/mesh/ico3.obj", "/Users/gregoireh/data/mesh/ico3.obj"]
extents = [[851, 849, 620], [177, 174, 116]]

for p, ext in zip(paths, extents):
    mesh = kalast.mesh.Mesh(p, update_pos=lambda x: x * ext)
    nfs.append(len(mesh.facets))
    meshes.append(mesh)

# vs = numpy.array([v.pos for v in meshes[0].vertices])

bods = [
    kalast.spice_entities.didymos,
    # kalast.spice_entities.dimorphos_pre,
    kalast.spice_entities.dimorphos,
]

spice.kclear()
spice.furnsh("/Users/gregoireh/data/spice/dart/mk/d520_v03.tm")

et_impact = spice.str2et("2022-09-26")
et_start = spice.str2et("2022-12-14 20:00:00")
et_end = spice.str2et("2022-12-15 08:00:00")

et = et_start
dt = 60 * 10

ets = []
dates = []
events = []
fluxes = [[], [], []]
mags = [[], [], []]

# bidirectional reflectance, should be computed for each facet
rbi = 1.0
albedo = 0.1


def compute_flux(
    facet: kalast.mesh.Facet,
    sun: numpy.ndarray,
    obs: numpy.ndarray,
    oth_mesh: kalast.mesh.Mesh | None = None,
    pos_from_oth: numpy.ndarray | None = None,
    to_oth: numpy.ndarray | None = None,
    radius_oth: float | None = None,
    iif: int | None = None,
) -> tuple[float, int]:
    # return int: 0 no intercept, 1 eclipse, 2 occultation
    p = facet.p
    n = facet.n.astype(numpy.float64)

    area = facet.a

    v_sun = sun - p
    d_sun = numpy.linalg.norm(sun)
    dau_sun = d_sun / kalast.util.AU
    u_sun = v_sun / d_sun

    if oth_mesh is not None:
        sun_from_oth = pos_from_oth + to_oth @ sun
        obs_from_oth = pos_from_oth + to_oth @ obs
        # d_sun_from_oth = numpy.linalg.norm(sun_from_oth)

        v_oth_from_oth = -pos_from_oth
        u_oth_from_oth = v_oth_from_oth / numpy.linalg.norm(v_oth_from_oth)
        v_sun_from_oth = sun_from_oth - pos_from_oth
        v_obs_from_oth = obs_from_oth - pos_from_oth
        u_sun_from_oth = v_sun_from_oth / numpy.linalg.norm(v_sun_from_oth)
        u_obs_from_oth = v_obs_from_oth / numpy.linalg.norm(v_obs_from_oth)
        cos_ang_vec_sun = kalast.astro.cosine_angle_vectors(
            u_oth_from_oth, u_sun_from_oth
        )
        cos_ang_vec_obs = kalast.astro.cosine_angle_vectors(
            u_oth_from_oth, u_obs_from_oth
        )

        p_from_oth = pos_from_oth + to_oth @ p
        ray_from_sun_to_p_from_oth = p_from_oth - sun_from_oth
        ray_from_obs_to_p_from_oth = p_from_oth - obs_from_oth

        apxv = numpy.cross(-sun_from_oth, ray_from_sun_to_p_from_oth)
        d_oth_to_ray_sun = numpy.linalg.norm(apxv) / numpy.linalg.norm(
            ray_from_sun_to_p_from_oth
        )

        apxv = numpy.cross(-obs_from_oth, ray_from_obs_to_p_from_oth)
        d_oth_to_ray_obs = numpy.linalg.norm(apxv) / numpy.linalg.norm(
            ray_from_obs_to_p_from_oth
        )

        # if True:
        if cos_ang_vec_obs > 0.0 and (
            radius_oth is None or d_oth_to_ray_obs < radius_oth
        ):
            out = oth_mesh.intersect(
                obs_from_oth,
                ray_from_obs_to_p_from_oth,
                exit_first=True,
            )

            if iif is not None and out is not None:
                print(f"f#{iif}: {out}")

            if out is not None:
                return 0.0, 2

        # if d_sun < d_sun_from_oth:
        # if True:
        if cos_ang_vec_sun > 0.0 and (
            radius_oth is None or d_oth_to_ray_sun < radius_oth
        ):
            out = oth_mesh.intersect(
                sun_from_oth,
                ray_from_sun_to_p_from_oth,
                exit_first=True,
            )

            if iif is not None and out is not None:
                print(f"f#{iif}: {out}")

            if out is not None:
                return 0.0, 1

    cosi = kalast.astro.cosine_incidence(u_sun, n)

    v_obs = obs - p
    d_obs = numpy.linalg.norm(v_obs)
    u_obs = v_obs / d_obs
    cose = kalast.astro.cosine_incidence(u_obs, n)

    return (
        rbi
        * kalast.util.SFLUX_545
        * albedo
        * cosi
        * area
        * cose
        / (dau_sun**2 * d_obs**2)
    ), 0


t0 = time.time()

while True:
    date = spice.timout(et, kalast.util.TIMOUT3)
    print(date, end="")

    ev = ""

    # (pos_from_oth, _lt) = spice.spkpos(
    #     bods[0].name, et, bods[1].frame, "none", bods[1].name
    # )
    # pos_from_oth *= 1e3
    # to_oth = spice.pxform(bods[0].frame, bods[1].frame, et)

    for iib in [0, 1]:
        iib2 = (iib + 1) % 2

        (pos_from_oth, _lt) = spice.spkpos(
            bods[iib].name, et, bods[iib2].frame, "none", bods[iib2].name
        )
        pos_from_oth *= 1e3
        to_oth = spice.pxform(bods[iib].frame, bods[iib2].frame, et)

        (sun, _lt) = spice.spkpos("sun", et, bods[iib].frame, "none", bods[iib].name)
        sun *= 1e3
        d_sun = numpy.linalg.norm(sun)
        dau_sun = d_sun / kalast.util.AU

        (obs, _lt) = spice.spkpos("earth", et, bods[iib].frame, "none", bods[iib].name)
        obs *= 1e3
        d_obs = numpy.linalg.norm(obs)
        dau_obs = d_obs / kalast.util.AU

        # if iib == 1:
        #     to_oth = numpy.linalg.inv(to_oth)
        #     pos_from_oth = to_oth @ -pos_from_oth

        # all_flux = Parallel(n_jobs=1)(
        #     delayed(compute_flux)(meshes[iib], sun, obs, iif)
        #     for iif in range(0, meshes[iib].faces.shape[0])
        # )

        all_flux = [
            compute_flux(
                meshes[iib].facets[iif],
                sun,
                obs,
                oth_mesh=meshes[iib2],
                pos_from_oth=pos_from_oth,
                to_oth=to_oth,
                radius_oth=bods[iib2].radius * 1.5,
                # iif=iif,
            )
            for iif in range(nfs[iib])
        ]

        sum_flux = sum(flux[0] for flux in all_flux)
        ecl = sum(1 for flux in all_flux if flux[1] == 1)
        occ = sum(1 for flux in all_flux if flux[1] == 2)

        which_body = "P" if iib == 0 else "S"
        if occ > 0:
            if ev != "":
                ev += "+"
            ev += f"{which_body}O({occ}/{nfs[iib]})"
        if ecl > 0:
            if ev != "":
                ev += "+"
            ev += f"{which_body}E({ecl}/{nfs[iib]})"

        mag = apparent_reduced_vmag(sum_flux, dau_sun * dau_obs)
        fluxes[iib].append(sum_flux)
        mags[iib].append(mag)

    ets.append(et)
    dates.append(date)
    events.append(ev)
    flux = fluxes[0][-1] + fluxes[1][-1]
    mag = apparent_reduced_vmag(flux, dau_sun * dau_obs)
    fluxes[2].append(flux)
    mags[2].append(mag)

    if ev != "":
        print(f" {ev}", end="")

    print()

    et += dt

    if et > et_end:
        break

spice.kclear()

t1 = time.time()
print(f"time loop computation time: {t1 - t0:.6f}s")

ets = numpy.array(ets)
t = ets - ets[0]
t_fn = numpy.linspace(t[0], t[-1], num=10000, endpoint=True)

flux3 = numpy.array(fluxes[2])
mag3 = numpy.array(mags[2])
events = numpy.array(events)
idx_ev = numpy.where(events != "")[0]
idx_noev = numpy.where(events == "")[0]
t_noev = t[idx_noev]
flux3_noev = flux3[idx_noev]
mag3_noev = mag3[idx_noev]
ets_noev = ets[idx_noev]

varx, vary = symfit.variables("x, y")
# f, = symfit.parameters('f')
f1 = 2.0 * numpy.pi / bods[0].spin_period
f2 = 2.0 * numpy.pi / bods[1].spin_period
fit_model = {vary: kalast.util.fourier_series(varx, f=f1, n=4, ss="1")}  # 20
print(fit_model)

fit = symfit.Fit(fit_model, x=t_noev, y=mag3_noev)
fit_result = fit.execute()
mag3_fit = fit.model(x=t, **fit_result.params).y
mag3_fit_fn = fit.model(x=t_fn, **fit_result.params).y
mag3_fit_diff = mag3_fit - mag3
mag3_fit_err = ((mag3_fit_diff) ** 2)[idx_noev].sum() / idx_noev.size


df = {}
df["et"] = ets
df["date"] = dates
df["event"] = events
df["flux1"] = fluxes[0]
df["flux2"] = fluxes[1]
df["flux3"] = fluxes[2]
df["mag1"] = mags[0]
df["mag2"] = mags[1]
df["mag3"] = mags[2]
df["mag4"] = mag3_fit
df["mag5"] = mag3_fit_diff
df = pandas.DataFrame(df)
df.to_csv("out/flux.csv", index=False, encoding="utf-8-sig")


t_lbl = "hr"
t_fac = 3600.0
t /= t_fac
t_fn /= t_fac
t_noev /= t_fac
lim_auto = True

kalast.plot.style.load("paper")
fig, axes = pyplot.subplots(2, 1, figsize=(11.0, 4.0))
fig.supylabel("flux (10^-14 w/m2/nm)")
ax = axes[0]
ax.scatter(t, flux3 * 1e14, s=10, marker="o", fc="k")
if lim_auto:
    ax.set_xlim(0, t[-1])
else:
    ax.set_xlim(0, 180)
    ax.set_ylim(2.3, 2.7)
# ax.set_yticks([1.4, 1.5, 1.6, 1.7])
ax.text(
    0.008, 0.07, "a", size=16, fontname="arial", weight="black", transform=ax.transAxes
)

ax = axes[1]
ax.set_xlabel(f"elapsed time ({t_lbl})")
# ax.scatter(t, (df["flux3"] - df["flux1"]) * 1e14, s=10, marker="o", fc="k")
ax.scatter(t, df["flux2"] * 1e14, s=10, marker="o", fc="k")
if lim_auto:
    ax.set_xlim(0, t[-1])
else:
    ax.set_xlim(0, 180)
    ax.set_ylim(-0.05, 0.15)
    # ax.set_yticks([-0.05, 0.0, 0.05, 0.1])
    kalast.plot.style.use_formatter_decimal_round(ax)
ax.text(
    0.008, 0.07, "b", size=16, fontname="arial", weight="black", transform=ax.transAxes
)
fig.savefig("out/flux.png")

fig, axes = pyplot.subplots(2, 1, figsize=(11.0, 4.0))
fig.supylabel("Reduced V-band Apparent Magnitude")
ax = axes[0]
ax.scatter(t, df["mag3"], s=10, marker="o", fc="k")
# ax.scatter(t_noev, mag3_noev, s=10, marker="o", fc="r")
ax.plot(t_fn, mag3_fit_fn, lw=1, color="k")
# ax.plot(t, df["mag1"] - df["mag1"].mean(), lw=1, color="k")
# ax.set_xlim(0, 2 * bods[0].spin_period / 3600.0)
if lim_auto:
    ax.set_xlim(0, t[-1])
    ax.yaxis.set_inverted(True)
else:
    ax.set_xlim(0, 180)
    ax.set_ylim(10.8, 10.5)
    ax.set_yticks([10.8, 10.7, 10.6, 10.5])
ax.text(
    0.008, 0.07, "a", size=16, fontname="arial", weight="black", transform=ax.transAxes
)

ax = axes[1]
ax.set_xlabel(f"Elapsed time ({t_lbl})")
# ax.scatter(t, df["mag3"] - df["mag1"], s=10, marker="o", fc="k")
# ax.scatter(t, df["mag2"], s=10, marker="o", fc="k")
ax.scatter(t, mag3_fit_diff, s=10, marker="o", fc="k")
if lim_auto:
    ax.set_xlim(0, t[-1])
    # ax.yaxis.set_inverted(True)
else:
    ax.set_xlim(0, 180)
    ax.set_ylim(0.05, -0.1)
    ax.set_yticks([0.05, 0.0, -0.05, -0.1])
ax.text(
    0.008, 0.07, "b", size=16, fontname="arial", weight="black", transform=ax.transAxes
)
# kalast.plot.style.use_formatter_decimal_round(ax)
# ax.legend()
fig.savefig("out/mag.png")
pyplot.show()
