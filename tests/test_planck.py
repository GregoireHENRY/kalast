#!/usr/bin/env python
"""Planck's law against the two laws that follow from it exactly.

`src/tpm/emit.rs` is now the only implementation. `kalast/tpm/radiance.py`
used to carry a second one in numpy, with `h`, `c` and `k_B` redefined
locally, which is the duplication `notes/2026-09-10_code_quality_audit.md`
flagged: two closed forms agreeing only by luck, while `kalast.util` already
re-exported those constants from Rust. Testing the one that survived therefore
validates both front doors at once, and the last check pins them together.

Wien and Stefan-Boltzmann are the right checks because neither is a
restatement of the formula. Both are *consequences* of it that involve
constants the formula does not mention -- Wien's `b` comes out of solving
`x = 5(1 - e^-x)`, and `sigma` out of `2 pi^5 k^4 / (15 h^3 c^2)`. So an error
in `h`, `c` or `k_B`, or a wrong power of the wavelength, moves them
immediately, where a self-consistency check would not notice.

The engine builds `Float = f32` deliberately, so tolerances are set from what
single precision delivers rather than from what the mathematics does. Where
that is the binding constraint it is said so.
"""

import sys

import numpy

from kalast.tpm import emit
from kalast.tpm import radiance
from kalast.util import (
    BOLTZMANN_CONSTANT,
    PLANK_CONSTANT,
    SPEED_LIGHT,
    STEFAN_BOLTZMANN,
)

# CODATA, and deliberately written out here rather than derived from the
# constants above: a test that computes its own expectation from the same
# inputs as the code cannot detect a wrong input.
WIEN_B = 2.897771955e-3  # m K

failures: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


TEMPS = (100.0, 200.0, 300.0, 500.0, 1000.0, 5778.0)

# --- 1. Wien's displacement law ------------------------------------------
# lambda_peak * T = b. Found by golden-section on a log-spaced grid rather
# than by argmax alone, so the answer is not limited by the grid spacing.


def peak_wavelength(t):
    lo, hi = numpy.log(1e-9), numpy.log(1e-2)
    phi = (numpy.sqrt(5.0) - 1.0) / 2.0
    a, b = lo, hi
    c, d = b - phi * (b - a), a + phi * (b - a)
    for _ in range(200):
        if radiance.planck(t, numpy.exp(c)) > radiance.planck(t, numpy.exp(d)):
            b = d
        else:
            a = c
        c, d = b - phi * (b - a), a + phi * (b - a)
    return float(numpy.exp((a + b) / 2.0))


worst_wien = 0.0
for T in TEMPS:
    rel = abs(peak_wavelength(T) * T / WIEN_B - 1.0)
    worst_wien = max(worst_wien, rel)

# The peak is quadratically flat, so locating it in f32 is limited by the
# ~1e-7 resolution of the value, not by the search: a 1e-7 error in B moves
# the argmax by ~sqrt(1e-7).
check(
    "test_wien_displacement_law",
    worst_wien < 1e-3,
    f"worst |lambda_peak * T / b - 1| = {worst_wien:.2e} over {len(TEMPS)} temperatures",
)

# --- 2. Stefan-Boltzmann -------------------------------------------------
# pi * integral of the spectral radiance over all wavelengths = sigma T^4.
# The pi is the cosine-weighted integral over the hemisphere: Planck is per
# steradian, Stefan-Boltzmann is a flux.
#
# Integrated on a log grid, which is what makes a decade-spanning integrand
# tractable: dI = B * lambda * d(ln lambda).


def total_flux(t):
    lnw = numpy.linspace(numpy.log(1e-8), numpy.log(1e-1), 20001)
    w = numpy.exp(lnw)
    b = radiance.planck(numpy.full(w.size, t), w)
    return float(numpy.pi * numpy.trapezoid(b * w, lnw))


worst_sb = 0.0
for T in TEMPS:
    rel = abs(total_flux(T) / (STEFAN_BOLTZMANN * T**4) - 1.0)
    worst_sb = max(worst_sb, rel)

check(
    "test_stefan_boltzmann_from_integrating_planck",
    worst_sb < 2e-3,
    f"worst |pi * integral(B) / (sigma T^4) - 1| = {worst_sb:.2e} over {len(TEMPS)} temperatures",
)

# --- 3. the two asymptotic limits ----------------------------------------
# Rayleigh-Jeans at long wavelength and the Wien tail at short: each drops a
# different term of the formula, so between them they pin both.

T = 300.0
long_w = numpy.array([2e-3, 5e-3, 1e-2, 5e-2])
rj = 2.0 * SPEED_LIGHT * BOLTZMANN_CONSTANT * T / long_w**4
dev = radiance.planck(numpy.full(long_w.size, T), long_w) / rj - 1.0

# Not "close to Rayleigh-Jeans at some wavelength I picked": Planck sits below
# it by a definite amount, and the *rate* is the testable thing. With
# x = hc/(lambda k T), B/B_RJ = x/(e^x - 1) = 1 - x/2 + O(x^2), so the
# fractional shortfall must approach x/2. At 2 mm and 300 K that is already
# 1.2e-2 -- a first attempt at this check asserted 5e-3 and failed against
# correct code, which is worth remembering: the deviation is physics, not
# error, and only its rate distinguishes a right formula from a wrong one.
x = PLANK_CONSTANT * SPEED_LIGHT / (long_w * BOLTZMANN_CONSTANT * T)
ratio = numpy.abs(dev) / (x / 2.0)
# The next term is x^2/12, so ratio = 1 - x/6 + ...; x <= 0.024 here.
check(
    "test_rayleigh_jeans_limit_at_long_wavelength",
    bool(numpy.all(dev < 0)) and numpy.abs(ratio - 1.0).max() < 0.02,
    "shortfall/(x/2) = " + ", ".join(f"{r:.4f}" for r in ratio)
    + f" at {long_w[0] * 1e3:.0f}-{long_w[-1] * 1e3:.0f} mm",
)

short_w = numpy.array([2e-6, 3e-6, 4e-6])
hc_kt = PLANK_CONSTANT * SPEED_LIGHT / (short_w * BOLTZMANN_CONSTANT * T)
wien_tail = (
    2.0 * PLANK_CONSTANT * SPEED_LIGHT**2 / short_w**5 * numpy.exp(-hc_kt)
)
rel_wt = numpy.abs(radiance.planck(numpy.full(3, T), short_w) / wien_tail - 1.0).max()
check(
    "test_wien_tail_at_short_wavelength",
    rel_wt < 1e-5,
    f"worst {rel_wt:.2e} at 2-4 um, 300 K",
)

# --- 4. monotonicity in temperature --------------------------------------
# A black body is brighter at every wavelength when it is hotter. Cheap, and
# it catches a sign or an inversion that the integral checks could average
# away.
w_probe = numpy.array([1e-6, 5e-6, 11e-6, 50e-6, 1e-3])
curves = numpy.array(
    [radiance.planck(numpy.full(w_probe.size, t), w_probe) for t in TEMPS]
)
check(
    "test_planck_increases_with_temperature_at_every_wavelength",
    bool(numpy.all(numpy.diff(curves, axis=0) > 0)),
    f"checked {len(TEMPS)} temperatures x {w_probe.size} wavelengths",
)

# --- 5. the two front doors are one formula ------------------------------
# The point of the merge. `radiance.planck` broadcasts and calls
# `emit.planck_array`; `emit.planck` is the scalar. They must agree bit for
# bit, because they are the same code -- not merely to a tolerance, which is
# what the previous two-implementation arrangement could offer.
pairs = [(t, w) for t in TEMPS for w in (2e-6, 8e-6, 11e-6, 14e-6, 1e-4)]
scalar = numpy.array([emit.planck(t, w) for t, w in pairs])
vector = radiance.planck(
    numpy.array([t for t, _ in pairs]), numpy.array([w for _, w in pairs])
)
check(
    "test_the_scalar_and_array_paths_are_bit_identical",
    bool(numpy.array_equal(scalar, vector)),
    f"{len(pairs)} (T, lambda) pairs agree exactly",
)

sys.exit(1 if failures else 0)
