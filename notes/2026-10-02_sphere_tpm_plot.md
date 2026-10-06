# 2026-10-02 — `examples/sphere/tpm_plot.py`: a sphere TPM run, drawn

Asked: a quick script, run after any of the sphere TPM scripts, showing the
surface temperature at different latitudes over spins and time, the depth
profile's evolution, the effect of obliquity where there is one, and of the
properties varying over the surface and with depth.

A script run in the editor gets a fresh namespace (`kalast/editor.py`) and
its folder is not on `sys.path`, so a run hands over through a file: each
TPM script records the columns under the facets of the meridian at longitude
0, pole to pole every 15 degrees -- the nearest facet to each, from the mesh's
facet centres -- and saves them when its loop ends, which a next script's
start makes happen first: `out/sphere/<script>.npz` (`time`, `temperature`
as samples x layers x latitudes in float32, `latitude`, `depth`, `period`,
`inertia`, `albedo`, and `obliquity`, `year` where they apply). 24 samples a
spin, at the same steps of every spin (`iteration * 24 % steps_per_spin <
24`); `tpm_logo.py`, a frame a spin, every 10 spins. About 64 KB a spin for
`tpm.py`. The graded column's layer widths come back from the conductivity it
carries, `w = dz k / conductivity`, without a change to the engine.

`tpm_plot.py` draws the newest file with matplotlib's Agg backend -- a window
of its own beside winit's in the editor's process was not worth the risk --
into a PNG beside it, which it opens with the system's viewer: the surface
through the run (north solid, south dashed, coloured by latitude), the mean
over each spin by latitude, the last spin pole to pole beside its mirror image,
the equator's column at one moment of each spin, the last spin's band at depth
at 30 deg north and south, and the thermal inertia down the meridian. The
meridian crosses `tpm_variable_1.py`'s bright patch and `tpm_variable_2.py`'s
largest dust pond, so their effects show against the mirrored latitude.

Checked on copies in /tmp, 20 spins each (the logo 0.3 years), cwd with a
`res` link so nothing went to the repository's `out/`: the figures show
symmetry for `tpm.py`, the solstice's asymmetry and the dark south cap for
`tpm_obliquity.py`, the bright patch cooler than its mirror for
`tpm_variable_1.py`, the dust pond's wider swing that damps faster with depth
for `tpm_variable_2.py`, and the logo's heating from 0 K and seasons.

## The meridian on any shape

Asked what happens when the mesh is not a sphere, or is resized. The first pick,
`argmax(centres @ direction)`, weighed each facet by its distance from the
centre -- an elongated body's far facets win off the meridian -- and was made
on the unstretched mesh, so `tpm_logo.py`'s `extents` moved every facet's true
latitude: asked 30, 45 and 60, its picks sat at 23.3, 40.3 and 52.2 on the
flattened body, and were labelled 30, 45 and 60. Now every script picks by
direction (`up`, the centres normalised), on the centres as the pose stretches
them (`@ extents` in the logo; a shape changed through its vertices already
is), drops a facet picked twice, and saves the latitudes the facets have:
29.7, 44.1 and 56.5 there; on `tpm.py`'s sphere the same facets as before, now
labelled -2.1 rather than 0 at the equator, which `ico4` has no facet on.
A concave body can be crossed more than once in one direction; the pick is
then whichever facet points most nearly along it.

## The figure, reworked (5 October)

The user: the surface through the whole run was unreadable, the latitude
heatmap and the inertia panel not wanted, the pole-to-pole panel wanted with
latitude up and its min-to-max band shaded by distribution, and the column
panel read as not converged. Now four panels: the surface over the last three
spins (the whole run where a sample is ten spins, the logo); the last spin pole
to pole as the time spent at each temperature, the 24 samples interpolated
round the spin and binned per latitude, transparent to black, with the mean
and its mirror -- darkest at the two ends, where the surface lingers at night
and around noon, not at the mean, which it passes quickly; the equator's column
spin after spin, titled with the most any layer moved over the last ten
samples; the daily wave at depth at 30 deg north and south. The user's 202-spin
`tpm.py` run had converged: +0.000 K, the bottom layer still moving 0.32 K a
spin between spins 10 and 20. Measured on the bottom layer alone, a short logo
run read +0.000 K too, its 12 m bottom not yet reached: hence the whole column.
The albedo axis only where the albedo varies.

Then: the first panel two spins; the 30 deg north-and-south bands replaced by
the equator's column at eight moments through the last spin, a cyclic colour
round the day. The time-at-temperature shading questioned -- "more data near
the mean?" -- and checked on the 202-spin run: the equator spends 40.7 % of
its last spin in the coldest sixth of its range, 22.4 % in the hottest, 7.1 %
in the sixth holding the mean; a periodic curve lingers at its extremes.
The poles' columns added to the last panel, the same colour a moment, the
north dashed and the south dotted: in `tpm.py` both at 130-145 K, their small
daily swing gone by 2.5 cm.

Then the pole-to-pole panel removed, the surface panel across the top in its
place, and the poles added to the spin-after-spin panel too, its title's
measure now over the three columns. On the user's 202-spin `tpm.py` run the
poles had settled as well, +0.000 K over the last 10 spins. On a 20-spin
`tpm_variable_1.py` test run they were still cooling, 34.6 K over the last 10
spins, from the whole sphere's mean temperature every facet starts at there
-- which the equator alone, settled within a few spins, had hidden. Nothing
reads the saved runs' `inertia` and `albedo` any more.

For a run with an orbit, the logo, the first panel was the whole run, 30
years once it ran them. The user: its last two spins and its last two years;
in the bottom right, several curves for the year as for the spin, as two
panels, the spin's and the year's; the bottom left untouched. The logo now
keeps its last two spins apart, 24 samples a spin in a deque of 48, beside its
sample every 10 spins (`spin_time`, `spin_temperature`), whenever the run
stops. Its figure is 16 inches wide: the surface over the last two spins and
the last two years side by side; below, the columns spin after spin as
before, through the last spin -- shown down to 1.5 times the depth where the
day's swing falls under 1 % of the surface's, the column being the year's,
11 m -- and through the last year. The spins' axis keeps whole numbers
(`useOffset=False`): the logo's last two are near spin 247,800. The other
runs keep their figure; the last spin's 8 moments are picked by time now, so
they may start at another moment of it.
