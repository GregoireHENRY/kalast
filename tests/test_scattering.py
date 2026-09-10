#!/usr/bin/env python
"""The optical scattering laws, against the identities that define them.

`src/scattering.rs` is the reflected-sunlight half of the photometry, added so
a ground-based light curve can be computed at all -- the thermal half already
existed in `tpm::emit`. Its Rust unit tests cover the algebra and the limits.
This covers the two things a unit test cannot, both of which are *defining*
properties rather than restatements of the code:

1. **The Chandrasekhar `H` function satisfies its integral equation.**
   `h_function` is Hapke's 2002 rational approximation to a function with no
   closed form. The only honest check is to put it back into

       H(x) = 1 + (w x / 2) H(x) * integral_0^1 H(u) / (x + u) du

   and see how far off it is. That is a real test of an approximation; a
   comparison against a hard-coded table would only test the transcription.

2. **The particle phase function is normalised.** A phase function must average
   to 1 over the sphere or it is inventing or destroying light, and every
   Hapke reflectance is proportional to it. This catches the classic
   Henyey-Greenstein sign error -- writing the lobes in the scattering angle
   while calling the argument the phase angle -- which leaves the
   normalisation intact but points the asymmetry the wrong way, so the third
   check tests the direction separately.

Pure numpy, no GPU and no window.
"""

import sys

import numpy

from kalast._rs import scattering as sc

failures: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


# --- 1. the H function against its own definition -------------------------
# Gauss-Legendre on [0, 1]: the integrand is smooth away from x = 0, and the
# 1/(x+u) is what makes a uniform rule converge slowly, so the nodes are
# placed rather than spaced.
NODES, WEIGHTS = numpy.polynomial.legendre.leggauss(400)
U = 0.5 * (NODES + 1.0)
W_QUAD = 0.5 * WEIGHTS


def h_residual(w, x):
    """|H(x) - (1 + w x H(x)/2 * int H(u)/(x+u) du)| / H(x)."""
    hx = sc.h_function(w, x)
    hu = numpy.array([sc.h_function(w, float(u)) for u in U])
    integral = float((W_QUAD * hu / (x + U)).sum())
    rhs = 1.0 + 0.5 * w * x * hx * integral
    return abs(hx - rhs) / hx


worst = 0.0
worst_at = None
for w in (0.05, 0.2, 0.5, 0.8, 0.95, 0.99):
    for x in (0.05, 0.2, 0.5, 0.8, 1.0):
        r = h_residual(w, x)
        if r > worst:
            worst, worst_at = r, (w, x)

# Hapke's 2002 form is quoted as better than 1 %; the 1993 form
# (1+2x)/(1+2x sqrt(1-w)) is about 4 %. Anything near 4 % here would mean the
# older approximation had been transcribed by mistake.
check(
    "test_h_function_satisfies_its_integral_equation",
    worst < 0.01,
    f"worst residual {worst:.2%} at w={worst_at[0]}, x={worst_at[1]}",
)

# It must also be exactly 1 with no scattering, and rise with albedo.
check(
    "test_h_function_is_unity_without_scattering",
    all(sc.h_function(0.0, x) == 1.0 for x in (0.0, 0.3, 1.0)),
)
check(
    "test_h_function_rises_with_albedo",
    all(
        sc.h_function(a, 0.7) < sc.h_function(b, 0.7)
        for a, b in zip((0.1, 0.3, 0.6), (0.3, 0.6, 0.9))
    ),
)

# --- 2. the phase function is normalised ---------------------------------
# (1/2) integral_0^pi P(a) sin a da = 1, the sphere average with the azimuth
# already done.
ANG = 0.5 * numpy.pi * (NODES + 1.0)
ANG_W = 0.5 * numpy.pi * WEIGHTS

worst_norm = 0.0
worst_norm_at = None
for b in (0.0, 0.2, 0.4, 0.6, 0.8):
    for c in (0.0, 0.25, 0.5, 0.75, 1.0):
        p = numpy.array([sc.henyey_greenstein(b, c, float(a)) for a in ANG])
        norm = 0.5 * float((ANG_W * p * numpy.sin(ANG)).sum())
        if abs(norm - 1.0) > worst_norm:
            worst_norm, worst_norm_at = abs(norm - 1.0), (b, c)

check(
    "test_phase_function_averages_to_one_over_the_sphere",
    worst_norm < 1e-6,
    f"worst |<P> - 1| = {worst_norm:.2e} at b={worst_norm_at[0]}, c={worst_norm_at[1]}",
)

# --- 3. and it points the right way --------------------------------------
# c is the *backward* fraction, and alpha = 0 is opposition, so c = 1 must be
# brightest at alpha = 0 and c = 0 brightest at alpha = pi. Normalisation
# alone cannot see this: swapping the lobes keeps the integral at 1.
back = sc.henyey_greenstein(0.5, 1.0, 0.0), sc.henyey_greenstein(0.5, 1.0, numpy.pi)
fwd = sc.henyey_greenstein(0.5, 0.0, 0.0), sc.henyey_greenstein(0.5, 0.0, numpy.pi)
check(
    "test_backscattering_lobe_faces_opposition",
    back[0] > back[1] and fwd[1] > fwd[0],
    f"c=1: {back[0]:.3f} at 0 vs {back[1]:.3f} at pi; "
    f"c=0: {fwd[0]:.3f} vs {fwd[1]:.3f}",
)
check(
    "test_isotropic_phase_function_is_flat",
    all(
        abs(sc.henyey_greenstein(0.0, 0.5, float(a)) - 1.0) < 1e-9
        for a in numpy.linspace(0, numpy.pi, 9)
    ),
)

# --- 4. the laws agree where they must -----------------------------------
# Hapke with no multiple scattering, no surge and isotropic particles *is*
# Lommel-Seeliger. The Rust side tests this too; repeated here because it is
# the property that holds all four laws to one mu0 convention, and it is the
# one a future edit is most likely to break from the Python side.
worst_red = 0.0
for w in (1e-6, 1e-4):
    h = sc.Hapke(w=w, b=0.0, c=0.0, b0=0.0, h=0.0)
    for mu0, mu in ((0.9, 0.7), (0.4, 0.4), (0.2, 0.95)):
        got = h.reflectance(mu0, mu, 0.4)
        want = sc.lommel_seeliger(w, mu0, mu)
        worst_red = max(worst_red, abs(got / want - 1.0) / w)
check(
    "test_hapke_reduces_to_lommel_seeliger_as_albedo_vanishes",
    worst_red < 3.0,
    f"residual/w = {worst_red:.2f} (must be O(1): the leftover is O(w))",
)

# --- 5. roughness is refused, not ignored --------------------------------
try:
    sc.Hapke(w=0.2, theta_bar=0.3).reflectance(0.5, 0.5, 0.2)
    check("test_unimplemented_roughness_raises", False, "no exception raised")
except ValueError as exc:
    check("test_unimplemented_roughness_raises", "theta_bar" in str(exc), str(exc)[:60])

# --- 6. brightness behaves ------------------------------------------------
h = sc.Hapke()
check(
    "test_reflectance_is_positive_and_finite_over_the_hemisphere",
    all(
        0.0 < h.reflectance(float(m0), float(m), float(al)) < 1.0
        for m0 in numpy.linspace(0.05, 1.0, 6)
        for m in numpy.linspace(0.05, 1.0, 6)
        for al in numpy.linspace(0.0, 2.5, 6)
    ),
)
check(
    "test_opposition_surge_brightens_toward_zero_phase",
    h.reflectance(0.7, 0.7, 0.0) > h.reflectance(0.7, 0.7, 0.3) > h.reflectance(0.7, 0.7, 1.2),
    f"{h.reflectance(0.7, 0.7, 0.0):.5f} > {h.reflectance(0.7, 0.7, 0.3):.5f} "
    f"> {h.reflectance(0.7, 0.7, 1.2):.5f}",
)

sys.exit(1 if failures else 0)
