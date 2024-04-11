import glm
import numpy
import itertools

RPD = numpy.pi / 180
DPR = 1 / RPD

axes = ["x", "y", "z"]
XYZ = numpy.eye(3)

AU_km = 1.495978707e8
solar_constant = 1367
stephan_boltzmann = 5.670374419e-8

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

tiri_id = -91200
tiri_pxx = 1024
tiri_pxy = 768
tiri_fovx = 13.0 * RPD
tiri_fovy = 10.0 * RPD

afc_pxx = 1024
afc_pxy = 1024
afc_fovx = 5.47 * RPD
afc_fovy = 5.47 * RPD

earth_orbit_period = 365.25 * 86400
earth_diameter = 12742

moon_orbit_period = 29.5 * 86400
moon_diameter = 3474.8

mars_id = 499
mars_orbit_period = 687 * 86400
mars_diameter = 6779
mars_radii = numpy.array([6378.1366, 6378.1366, 6356.7519])

phobos_orbit_period = 7 * 3600 + 39 * 60
phobos_diameter = 22.533

deimos_orbit_period = 30.312 * 3600
deimos_diameter = 12.4


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

def flatten(l: list) -> list:
    return list(itertools.chain.from_iterable(l))