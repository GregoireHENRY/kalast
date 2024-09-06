import glm
import numpy
import itertools


# u1 & u2 are unit vectors
def angle_between(u1, u2):
    return numpy.arccos(numpy.clip(u1 @ u2, -1.0, 1.0))


def mat_angle_axis(angle: float, axis: numpy.array):
    m = glm.mat3(glm.rotate(angle, glm.vec3(axis)))
    return numpy.array(m.to_list())


def sign(x):
    return "-" if x < 0 else ""


def hava(x: float, y: float) -> tuple[str, str]:
    if x < 0:
        ha = "right"
    if x > 0:
        ha = "left"
    if y < 0:
        va = "top"
    if y > 0:
        va = "bottom"
    return ha, va


def sha(ha: str) -> float:
    return 1 if ha == "left" else -1


def sva(ha: str) -> float:
    return 1 if ha == "bottom" else -1


def flatten(thelist: list) -> list:
    return list(itertools.chain.from_iterable(thelist))


def find_closest(
    m: numpy.ndarray,
    refv1: float,
    i1: int,
    threshold: int,
    refv2: float,
    i2: int,
    N: int = 1,
) -> list[tuple[int, numpy.ndarray]]:
    ii = numpy.where(numpy.abs(refv1 - m[:, i1]) < threshold)[0]
    jj = numpy.argsort(numpy.abs(refv2 - m[ii, i2]))[:N]
    kk = ii[jj]
    return [(kkk, m[kkk]) for kkk in kk]


def cart2sph(x, y, z):
    hxy = numpy.hypot(x, y)
    r = numpy.hypot(hxy, z)
    el = numpy.arctan2(z, hxy)
    az = numpy.arctan2(y, x)
    return numpy.array([az, el, r])


def sph2cart(az, el, r):
    rcos_theta = r * numpy.cos(el)
    x = rcos_theta * numpy.cos(az)
    y = rcos_theta * numpy.sin(az)
    z = r * numpy.sin(el)
    return numpy.array([x, y, z])


# value between 0 and 1
def cmapv_to_rbg(value, cmap):
    index = int(value * 255)
    return cmap.colors[index]


def flattening_radii(r3: numpy.ndarray) -> float:
    return (r3[0] - r3[2]) / r3[0]


def radius(r3: numpy.ndarray) -> float:
    return r3.mean()


RPD = numpy.pi / 180
DPR = 1 / RPD

axes = ["x", "y", "z"]
XYZ = numpy.eye(3)

AU_km = 1.495978707e8
solar_constant = 1367
stephan_boltzmann = 5.670374419e-8

minute = 60
hour = 3600
day = 86400

fmt_time_spice = "YYYY-MM-DD HR:MN:SC.### ::RND ::UTC"

# LPO: Mars ~1% fovcov ~9h30mn before flyby (~7800 pixels)
#      Mars ~2.5% fovcov ~6h0mn before flyby
#      Mars ~5% fovcov ~4h0mn before flyby
#      Mars ~10% fovcov ~3h0mn before flyby
#      Mars ~25% fovcov ~2h0mn before flyby
#      Mars ~50% fovcov ~1h0mn before flyby
dates = dict(
    LPC_LAUNCH="2024-10-27 14:15:18.680",
    LPC_MGA="2025-03-16 19:14:50.8",
    LPO_LAUNCH="2024-10-07 16:27:53.417",
    LPO_MGA="2025-03-12 23:35:50.8",
    ARRIVAL="2026 DEC 02 01:59:59.999",
    HERA_START="2027 FEB 09 16:07:00.0 TDB",
    HERA_END="2027 JUL 24 20:06:00.0 TDB",
    JUVENTAS_START="2027 MAR 24 12:00:00.0 TDB",
    JUVENTAS_END="2027 MAR 26 16:01:00.0 TDB",
)

scenario = "LPC"

tiri = {}
tiri["id"] = -91200
tiri["pxx"] = 1024
tiri["pxy"] = 768
tiri["fovx"] = 13.0 * RPD
tiri["fovy"] = 10.0 * RPD
tiri["npx"] = tiri["pxx"] * tiri["pxy"]

afc = {}
afc["pxx"] = 1024
afc["pxy"] = 1024
afc["fovx"] = 5.47 * RPD
afc["fovy"] = 5.47 * RPD
afc["npx"] = afc["pxx"] * afc["pxy"]

earth = {}
earth["name"] = "EARTH"
earth["frame"] = "IAU_EARTH"
earth["orbit_period"] = 365.25 * 86400
earth["diameter"] = 12742
earth["radii"] = numpy.array([6378.1366, 6378.1366, 6356.751])
earth["flattening"] = flattening_radii(earth["radii"])

moon = {}
moon["name"] = "MOON"
moon["frame"] = "IAU_MOON"
moon["orbit_period"] = 29.5 * 86400
moon["diameter"] = 3474.8
moon["radii"] = numpy.array([1737.4, 1737.4, 1737.4])
moon["flattening"] = flattening_radii(moon["radii"])

mars = {}
mars["id"] = 499
mars["name"] = "MARS"
mars["frame"] = "IAU_MARS"
mars["orbit_period"] = 687 * 86400
mars["diameter"] = 6779
mars["radii"] = numpy.array([3396.19, 3396.19, 3376.2])
mars["flattening"] = flattening_radii(mars["radii"])

phobos = {}
phobos["name"] = "PHOBOS"
phobos["frame"] = "IAU_PHOBOS"
phobos["orbit_period"] = 7 * 3600 + 39 * 60
phobos["diameter"] = 22.533
phobos["radii"] = numpy.array([13.0, 11.4, 9.1])
phobos["flattening"] = flattening_radii(phobos["radii"])

deimos = {}
deimos["name"] = "DEIMOS"
deimos["frame"] = "IAU_DEIMOS"
deimos["orbit_period"] = 30.312 * 3600
deimos["diameter"] = 12.4
deimos["radii"] = numpy.array([7.8, 6.0, 5.1])
deimos["flattening"] = flattening_radii(deimos["radii"])
