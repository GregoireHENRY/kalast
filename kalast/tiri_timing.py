"""Empirical timing offset between the TIRI image labels and the geometry.

Simulated Deimos sits away from the observed Deimos by an amount that grows as
the range falls -- up to 352 px on the 2025-03-12 Mars swing-by sequence. It is
**not** a pointing error: fitting the 15 usable frames, the best constant
pointing offset still leaves 77 px rms, while a single time shift leaves 7 px.

    dt = -24.89 +/- 0.86 s

fitted on the seven frames where Deimos moves fast enough for the quotient to
mean anything (2.7 to 19.6 px/s). Applied as: evaluate the geometry at
`label epoch + OFFSET_S`.

**This is an empirical correction with no established cause.** It is kept in
its own module, and off by default, for that reason. Two facts stop it being
called a clock offset:

  - It matches no standard time-system difference. TAI-UTC is 37 s in 2025,
    GPS-UTC 18 s, TDB-UTC about 69 s.
  - Fitted per frame it drifts +3.8 s across the 140 s sequence, where a
    constant clock error would give zero.

Its magnitude does rule out an ephemeris error in either body: 24.9 s is 220 km
of along-track separation at the 8.84 km/s relative speed, and neither Deimos's
orbit nor a reconstructed Hera flyby trajectory is uncertain at that level. So
it points at the image timestamps -- but what they record (time system, and
whether the stamp is integration start, mid, end, or packet time) is a question
for the TIRI team, not something this module answers.

**Do not quote it as a measurement of anything physical**, and say it was
applied whenever a product built with it is circulated.

Mars cannot check it: Hera flies almost straight at Mars, so Mars grows rather
than translating (0.011-0.024 px/s), and 24.9 s moves it under one pixel.

See notes/2026-09-03_tiri_timing/.
"""

OFFSET_S = -24.89

# Off by default: an unexplained correction should be opted into, not inherited
# silently by every script that imports this.
ENABLED = False


def apply(et):
    """Epoch to evaluate the geometry at, for an image labelled `et`."""
    return et + OFFSET_S if ENABLED else et
