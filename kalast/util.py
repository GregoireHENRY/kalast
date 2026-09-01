# from kalast._util import (  # noqa
#     numpy_float,
#     diag3,
#     trace3,
#     find_rotang,
#     find_rotaxis,
#     matpow,
#     newton_method,
#     cmapv_to_rbg,
#     numdigits_all,
#     numdigits_comma,
#     cart2sph,
#     sph2cart,
#     glm_cart2sph,
#     glm_sph2cart,
#     flattening,
#     find_closest,
#     distance_haversine,
#     fourier_series,
#     TIMOUT1,
#     TIMOUT2,
#     TIMOUT3,
#     SFLUX_545,
#     mat_axis_angle,
# )

import numpy

from kalast._rs.util import (  # noqa
    EPSILON,
    HOUR,
    DAY,
    PI,
    DPR,
    RPD,
    AU,
    AU_KM,
    SOLAR_CONSTANT,
    STEFAN_BOLTZMANN,
    PLANK_CONSTANT,
    SPEED_LIGHT,
    BOLTZMANN_CONSTANT,
    TWO_C,
    HC,
    HC2,
    HC_PER_K,
    TWO_HC2,
    TEMP_SUN,
    RADIUS_SUN,
    JANSKY,
    BAND_V0,
    GRAVITATIONAL_CONSTANT,
    MASS_SUN,
    NEWTON_METHOD_MAX_ITERATION,
    NEWTON_METHOD_THRESHOLD,
    SPICE_PICTUR_1,
    SPICE_PICTUR_2,
    SPICE_PICTUR_3,
    SFLUX_545,
)


def mat_axis_angle(axis: numpy.ndarray, angle: float) -> numpy.ndarray:
    sin, cos = numpy.sin(angle), numpy.cos(angle)
    xsin, ysin, zsin = axis * sin
    x, y, z = axis
    x2, y2, z2 = axis**2
    omc = 1 - cos
    xyomc = x * y * omc
    xzomc = x * z * omc
    yzomc = y * z * omc
    return numpy.array(
        [
            [x2 * omc + cos, xyomc - zsin, xzomc + ysin],
            [xyomc + zsin, y2 * omc + cos, yzomc - xsin],
            [xzomc - ysin, yzomc + xsin, z2 * omc + cos],
        ]
    )


def numdigits_all(v: float) -> int:
    return numpy.floor(numpy.log10(v))


def numdigits_comma(v: float) -> int:
    d = numdigits_all(v)
    if v < 1.0:
        return abs(d)
    else:
        return 0


class Rate:
    """Smoothed iteration rate, formatted in whichever unit reads best.

    A long thermophysical run can sit anywhere from tens of iterations a
    second to a handful an hour -- a view-factor rebuild alone takes several
    seconds -- so a fixed unit is unreadable at one end or the other. This
    picks per second, per minute or per hour so the number stays above 1.

    Averaged over a sliding window rather than over the whole run, so the
    figure reflects what the loop is doing now: a run that spent its first
    minutes building view factors should not report that forever.

        rate = Rate()
        ...
        rate.tick()
        sim.hud = f"{i}/{n} it   {rate}"
    """

    def __init__(self, window=40):
        from collections import deque
        self._t = deque(maxlen=max(int(window), 2))

    def tick(self):
        """Record one iteration."""
        import time
        self._t.append(time.perf_counter())
        return self

    @property
    def per_second(self):
        """Iterations per second over the window, or None until it can tell."""
        if len(self._t) < 2:
            return None
        span = self._t[-1] - self._t[0]
        return (len(self._t) - 1) / span if span > 0 else None

    def text(self):
        r = self.per_second
        if r is None:
            return "-- it/s"
        if r >= 1.0:
            return f"{r:.1f} it/s"
        if r * 60.0 >= 1.0:
            return f"{r * 60.0:.1f} it/min"
        return f"{r * 3600.0:.1f} it/h"

    def eta(self, remaining):
        """`h:mm:ss` left at the current rate, or empty if not yet known."""
        r = self.per_second
        if r is None or r <= 0 or remaining <= 0:
            return ""
        s = int(remaining / r)
        return f"{s // 3600:d}:{(s % 3600) // 60:02d}:{s % 60:02d}"

    def __str__(self):
        return self.text()
