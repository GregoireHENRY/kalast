#!/usr/bin/env python

# import math
from math import pi

import numpy
import scipy

from matplotlib import pyplot
# import matplotlib

import kalast
from kalast.tpm.core import planck
from kalast.util import TEMP_SUN, RADIUS_SUN, AU


# loc = matplotlib.ticker.MultipleLocator(base=1)
# ax.yaxis.set_major_locator(loc)


# emissivity surface of asteroid
e = 0.9

# albedo surface
a = 0.1

# diameter of asteroid
D = 1e3

# distance from observer to asteroid
d = 1e3

# distance from Sun to asteroid
dau = 1.0 * AU

# wavelength observer
wu = numpy.linspace(1e-3, 1000.0, 100000)
w = wu * 1e-6

# planck functions
f5778 = planck(TEMP_SUN, w)
f400 = planck(400, w)
f300 = planck(300, w)
f200 = planck(200, w)
f100 = planck(100, w)

kalast.plot.style.load()
fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Wavelength [µm]")
ax.set_ylabel("Spectral radiance [W/m3/sr]")
ax.plot(wu, f5778, lw=1, ls="-", color="k")
ax.set_xlim(0.1, 3.0)
ax.set_ylim(0.0, None)
fig.savefig("emittance_5778.png", bbox_inches="tight", dpi=300)

kalast.plot.style.load()
fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Wavelength [µm]")
ax.set_ylabel("Spectral radiance [W/m3/sr]")
(l100,) = ax.plot(wu, f100, lw=1, ls="-.", color="k")
ax.set_xlim(0.1, 200.0)
ax.set_ylim(0, None)
pyplot.legend([l100], ["100K"])
fig.savefig("emittance_100.png", bbox_inches="tight", dpi=300)

kalast.plot.style.load()
fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Wavelength [µm]")
ax.set_ylabel("Spectral radiance [W/m3/sr]")
(l200,) = ax.plot(wu, f200, lw=1, ls=":", color="k")
ax.set_xlim(0.1, 100.0)
ax.set_ylim(0, None)
pyplot.legend([l200], ["200K"])
fig.savefig("emittance_200.png", bbox_inches="tight", dpi=300)

kalast.plot.style.load()
fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Wavelength [µm]")
ax.set_ylabel("Spectral radiance [W/m3/sr]")
(l300,) = ax.plot(wu, f300, lw=1, ls="--", color="k")
(l400,) = ax.plot(wu, f400, lw=1, ls="-", color="k")
ax.set_xlim(0.1, 40.0)
ax.set_ylim(0, None)
pyplot.legend([l400, l300], ["400K", "300K"])
fig.savefig("emittance_300_400.png", bbox_inches="tight", dpi=300)

kalast.plot.style.load()
fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Wavelength [µm]")
ax.set_ylabel("Spectral radiance [W/m3/sr]")
(l400,) = ax.plot(wu, f400, lw=1, ls="-", color="k")
(l300,) = ax.plot(wu, f300, lw=1, ls="--", color="k")
(l200,) = ax.plot(wu, f200, lw=1, ls=":", color="k")
(l100,) = ax.plot(wu, f100, lw=1, ls="-.", color="k")
ax.set_xlim(1, 1000.0)
ax.set_ylim(1e3, 1e8)
ax.set_xscale("log")
ax.set_yscale("log")
pyplot.legend([l400, l300, l200, l100], ["400K", "300K", "200K", "100K"])
fig.savefig("emittance_100_400_log.png", bbox_inches="tight", dpi=300)

kalast.plot.style.load()
fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Wavelength [µm]")
ax.set_ylabel("Spectral radiance [W/m3/sr]")
(l5778,) = ax.plot(wu, f5778, lw=1.5, ls="-", color="k")
(l400,) = ax.plot(wu, f400, lw=1, ls="-", color="k")
ax.set_xlim(1e-2, 1000.0)
ax.set_ylim(1e4, 1e14)
ax.set_xscale("log")
ax.set_yscale("log")
pyplot.legend([l5778, l400], ["5778K", "400K"])
fig.savefig("emittance_5778_400_log.png", bbox_inches="tight", dpi=300)

# same as kalast.tpm.core.solar_flux function
f_surf = pi * D * D / (4 * d * d)
f_ref = f_surf * a * f5778 * RADIUS_SUN * RADIUS_SUN / (dau * dau)
f_emi = f_surf * e * f400

kalast.plot.style.load()
fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Wavelength [µm]")
ax.set_ylabel("Spectral irradiance [W/m3]")
(l_sun,) = ax.plot(wu, f_ref, lw=1, ls="-", color="k")
(l400,) = ax.plot(wu, f_emi, lw=1, ls="--", color="k")
ax.set_xlim(1e-1, 100.0)
ax.set_ylim(1e4, 1e8)
ax.set_xscale("log")
ax.set_yscale("log")
pyplot.legend([l_sun, l400], ["reflectance", "emittance"])
fig.savefig("emittance_reflectance_sun_400_log.png", bbox_inches="tight", dpi=300)
# pyplot.show()

fn_f_sun = lambda x: planck(TEMP_SUN, x) * pi * RADIUS_SUN * RADIUS_SUN / (AU * AU)
S, _err = scipy.integrate.quad(fn_f_sun, 1e-10, 1e-2)
print(S)
