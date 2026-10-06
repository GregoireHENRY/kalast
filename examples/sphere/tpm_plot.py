#!/usr/bin/env python

import os
import pathlib
import subprocess
import sys

import matplotlib
import numpy

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402

# Draws the last run of a sphere TPM script -- tpm.py, tpm_variable_1.py,
# tpm_variable_2.py, tpm_obliquity.py or tpm_logo.py -- from what it saved when
# it stopped, out/sphere/<script>.npz: the columns under the facets of the
# meridian at longitude 0, pole to pole. Run it after one, or name the run.
runs = sorted(pathlib.Path("out/sphere").glob("*.npz"), key=lambda p: p.stat().st_mtime)
run = runs[-1]  # or pathlib.Path("out/sphere/tpm_obliquity.npz")
r = numpy.load(run)

T = r["temperature"]  # (samples, layers, latitudes) K
lat = r["latitude"]  # (degrees)
z = r["depth"] * 100.0  # layers (cm)
obliquity = float(r["obliquity"]) if "obliquity" in r else 0.0

# Time in spins, or years when the run has an orbit; samples grouped by spin
# when there are several a spin.
unit, scale = ("years", r["year"]) if "year" in r else ("spins", r["period"])
time = r["time"] / scale
per = int(round(r["period"] / numpy.median(numpy.diff(r["time"])))) if len(T) > 1 else 1
per = max(per, 1)
spins = len(T) // per
if spins == 0:
    sys.exit(f"{run} holds less than a spin: let the TPM script run longer")

# A run with an orbit samples it every few spins, and keeps its last two spins
# apart, 24 samples a spin (tpm_logo.py): the day and the year side by side.
orbit = "year" in r
day_time, day_T = (r["spin_time"], r["spin_temperature"]) if "spin_time" in r else (r["time"], T)

# The columns of the equator and the two poles, or of the facets nearest them,
# solid, dashed and dotted, at each of the samples rows, a colour a sample.
eq, north, south = numpy.argmin(abs(lat)), numpy.argmax(lat), numpy.argmin(lat)


def columns(a, T, rows, colors, labels):
    places = ((eq, "-", "equator"), (north, "--", "north pole"), (south, ":", "south pole"))
    for place, style, _ in places:
        for k, color, label in zip(rows, colors, labels):
            a.plot(T[k, :, place], z, style, color=color, label=label if place == eq else None)
    for place, style, name in places:
        a.plot([], [], style, color="0.3", label=f"{name}, {lat[place]:+.0f} deg")
    a.invert_yaxis()
    a.set(xlabel="temperature (K)", ylabel="depth (cm)")
    a.legend(fontsize=7)


# The surface temperature at each of the samples, a latitude a line, north
# solid and south dashed in one colour: the two hemispheres apart or together.
def surface(a, x, T, xlabel, title):
    for k in range(0, len(lat), 2):
        style = "-" if lat[k] >= 0 else "--"
        a.plot(x, T[:, 0, k], style, color=plt.cm.viridis(abs(lat[k]) / 90.0), label=f"{lat[k]:+.0f} deg")
    a.set(xlabel=xlabel, ylabel="surface temperature (K)", title=title)
    a.ticklabel_format(axis="x", useOffset=False)
    a.locator_params(axis="x", nbins=6)
    a.legend(fontsize=7, ncol=2)


title = f"{run.stem}: obliquity {obliquity:.0f} deg" if obliquity else run.stem
if orbit:
    layout = [["day"] * 3 + ["year"] * 3, ["spins"] * 2 + ["spin"] * 2 + ["orbit"] * 2]
else:
    layout = [["day", "day"], ["spins", "spin"]]
fig, ax = plt.subplot_mosaic(layout, figsize=(16 if orbit else 13, 10), constrained_layout=True)
fig.suptitle(f"{title}, {time[-1]:.4g} {unit}")

# The surface over the last two spins: the daily wave. Beside it, where there
# is an orbit, over the last two years: the seasons.
two = day_time > day_time[-1] - 2.0 * r["period"]
surface(ax["day"], day_time[two] / r["period"], day_T[two], "spins", "Surface, the last two spins")
if orbit:
    two = r["time"] > r["time"][-1] - 2.0 * r["year"]
    surface(ax["year"], time[two], T[two], "years", "Surface, the last two years")

# The columns at the same moment of each spin: the warmth soaking down as the
# run settles; the title says how much they still moved over the last 10
# samples, where they moved most -- next to nothing once the run has converged.
a = ax["spins"]
picks = numpy.unique(numpy.geomspace(1, spins, 8).astype(int)) - 1
columns(a, T, picks * per, plt.cm.viridis(numpy.linspace(0, 1, len(picks))), [f"{time[s * per]:.3g} {unit}" for s in picks])
back = max(spins - 11, 0)
moved = (T[(spins - 1) * per] - T[back * per])[:, [eq, north, south]]
drift = moved.flat[numpy.argmax(abs(moved))]
a.set_title(f"Columns spin after spin\nmoved {drift:+.3f} K at most over the last {time[(spins - 1) * per] - time[back * per]:.3g} {unit}")

# The columns at moments through the last spin, one colour a moment round the
# day: the daily wave going down, lagging and dying out with depth -- at the
# poles, what is left of it. A column made deep for a year's wave is shown as
# deep as the day's reaches.
a = ax["spin"]
last = numpy.flatnonzero(day_time > day_time[-1] - r["period"])
rows = last[:: max(len(last) // 8, 1)][:8]
labels = [f"+{(day_time[k] - day_time[rows[0]]) / r['period']:.3f} spin" for k in rows]
columns(a, day_T, rows, plt.cm.twilight(numpy.arange(len(rows)) / len(rows)), labels)
a.set_title("Columns through the last spin")
if orbit:
    swing = numpy.ptp(day_T[last], axis=0).max(axis=1)  # through the spin, each layer
    deep = z[swing > 0.01 * swing[0]]
    if len(deep):
        a.set_ylim(min(1.5 * deep.max(), z[-1]), 0.0)

# And through the last year: the seasons' wave going down the whole column.
if orbit:
    a = ax["orbit"]
    last = numpy.flatnonzero(r["time"] > r["time"][-1] - r["year"])
    rows = last[:: max(len(last) // 8, 1)][:8]
    labels = [f"+{(r['time'][k] - r['time'][rows[0]]) / r['year']:.3f} year" for k in rows]
    columns(a, T, rows, plt.cm.twilight(numpy.arange(len(rows)) / len(rows)), labels)
    a.set_title("Columns through the last year")

out = run.with_suffix(".png")
fig.savefig(out, dpi=110)
print(f"wrote {out}")
if sys.platform == "win32":
    os.startfile(out)
else:
    subprocess.Popen(["open" if sys.platform == "darwin" else "xdg-open", str(out)])
