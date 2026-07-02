from pathlib import Path

import numpy
import matplotlib  # noqa
from matplotlib import pyplot  # noqa

import kalast
from kalast.util import DPR, RPD, SPEED_LIGHT, JANSKY  # noqa


path = Path("out")
et = numpy.load(path / "et_dates.npy")
wl = numpy.load(path / "wl.npy")
spec_irrad = numpy.load(path / "spec_irrad.npy")
spec_irrad_jy = spec_irrad * wl**2 / SPEED_LIGHT * JANSKY
print(f"ntime={et.size}")

kalast.plot.style.load()

fig, ax = pyplot.subplots(figsize=(6, 4))
ax.set_xlabel("Wavelength [um]")
ax.set_ylabel("Spectral Irradiance [Jy]")
ax.plot(wl * 1e6, spec_irrad_jy[0, :], lw=1, color="k")
ax.set_xlim(0.9, 29)
ax.set_ylim(3e7, 2e11)
ax.set_yscale("log")
fig.savefig("spec_irrad_jy.png", bbox_inches="tight", dpi=300)
