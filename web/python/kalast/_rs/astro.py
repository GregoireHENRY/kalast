"""`kalast._rs.astro` in the page."""

from kalast._rs._page import _fn


class Orbit:
    """An orbit about the Sun, from its semi-major axis `a` (AU) and its
    eccentricity `e`, in its own plane: x toward perihelion."""

    def __init__(self, a: float, e: float) -> None:
        if not (a > 0.0 and 0.0 <= e < 1.0):
            raise ValueError(f"an orbit about the Sun wants a > 0 and 0 <= e < 1: not a = {a}, e = {e}")
        self.a, self.e = float(a), float(e)

    @property
    def period(self) -> float:
        """Its period (s), by Kepler's third law."""
        return _fn("orbit_period", self.a, self.e)

    def position(self, t: float):
        """Where the body is `t` seconds after perihelion, in AU."""
        return _fn("orbit_position", self.a, self.e, float(t))

    def __repr__(self) -> str:
        return f"Orbit(a={self.a}, e={self.e})"
