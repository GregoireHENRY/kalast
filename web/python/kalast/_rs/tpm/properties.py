"""`kalast._rs.tpm.properties` in the page."""

from kalast._rs._page import Properties, _fn, _preset  # noqa: F401

DIDYMOS = _preset("DIDYMOS")
DIMORPHOS = _preset("DIMORPHOS")
MOON = _preset("MOON")
PHOBOS = _preset("PHOBOS")
DEIMOS = _preset("DEIMOS")


def conductivity(ti: float, p: float, c: float) -> float:
    return _fn("conductivity", ti, p, c)


def diffusivity(k: float, p: float, c: float) -> float:
    return _fn("diffusivity", k, p, c)


def thermal_inertia(k: float, p: float, c: float) -> float:
    return _fn("thermal_inertia", k, p, c)


def skin_depth_1(d: float, p: float) -> float:
    return _fn("skin_depth_1", d, p)


def skin_depth_2pi(d: float, p: float) -> float:
    return _fn("skin_depth_2pi", d, p)
