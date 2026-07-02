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
