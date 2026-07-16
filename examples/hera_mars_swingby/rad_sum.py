#!/usr/bin/env python

# Sum radiance/irradiance of Deimos as seen by TIRI over time, from the
# per-facet output of rad.py (rad_all.csv, irrad_all.csv).
#
# Note on what "sum" means here:
#   - irrad_all is per-facet irradiance [W/m2] at the detector, i.e. already
#     an extensive quantity (it folds in the facet's projected area/distance
#     via steradian()). Summing it over facets gives a physically meaningful
#     total instantaneous flux received from the whole disk of Deimos.
#   - rad_all is per-facet radiance [W/m2/sr]. Radiance is intensive (an
#     angular brightness), not additive across facets in the same sense --
#     summing it doesn't correspond to a standard radiometric quantity of
#     the whole body. It's still reported here (as e.g. an average) since
#     it was asked for, but irrad_sum is the one to trust/compare against
#     real TIRI photometry.
#   - Facets facing away from TIRI (cose < 0 in rad.py) end up with negative
#     rad_/irrad_ values (rad.py does not clip them), which is unphysical --
#     a backside/self-occluded facet contributes zero flux, not negative.
#     They are clipped to 0 before summing here.

from pathlib import Path

import numpy
import pandas
from matplotlib import pyplot

import kalast

path = Path("out/hera_mars_swingby/deimos_tpm_3")
ets = pandas.read_csv(path / "ets_sim.csv")["time"].to_numpy()
rad_all = pandas.read_csv(path / "rad_all.csv").to_numpy()
irrad_all = pandas.read_csv(path / "irrad_all.csv").to_numpy()

# Clip negative (backside/self-occluded facet) contributions to zero.
rad_clipped = numpy.clip(rad_all, 0.0, None)
irrad_clipped = numpy.clip(irrad_all, 0.0, None)

rad_sum = rad_clipped.sum(axis=1)
irrad_sum = irrad_clipped.sum(axis=1)
rad_mean = rad_clipped.mean(axis=1)

df = pandas.DataFrame(
    {
        "time": ets,
        "rad_sum": rad_sum,
        "rad_mean": rad_mean,
        "irrad_sum": irrad_sum,
    }
)
df.to_csv(path / "rad_sum.csv", index=False, encoding="utf-8-sig")

kalast.plot.style.load()

fig, axs = pyplot.subplots(2, 1, figsize=(6, 6), sharex=True)

axs[0].plot(ets, irrad_sum, lw=1, color="k")
axs[0].set_ylabel("Sum irradiance [W/m2]")

axs[1].plot(ets, rad_sum, lw=1, color="k")
axs[1].set_ylabel("Sum radiance [W/m2/sr]")
axs[1].set_xlabel("Ephemeris time [s]")

fig.savefig(path / "rad_sum.png", bbox_inches="tight", dpi=300)

print(f"irrad_sum: mean={irrad_sum.mean():.4e} min={irrad_sum.min():.4e} max={irrad_sum.max():.4e}")
print(f"rad_sum:   mean={rad_sum.mean():.4e} min={rad_sum.min():.4e} max={rad_sum.max():.4e}")
