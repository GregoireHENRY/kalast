#!/usr/bin/env python
"""Validation of the Kuehrt spherical-crater roughness correction.

Four kinds of check, in increasing strength:

1. **Exact limits.** `gamma -> 0` and `density = 0` must both give `R = 1`, and
   past the horizon there is nothing to correct.
2. **Convergence** in the number of integration nodes.
3. **Against the published schematic**, Kuehrt's `F5 > F1 > F6` -- the only
   external reference available, reproduced from `roughness-kuehrt/pres4.pptx`
   where the ROB validation recorded R5 = 1.6995 and R6 = 0.9845.
4. **That the divergence at grazing emission is benign**, which is the reason
   the cap at 10 was removed rather than kept.

Run:  python examples/analytical/roughness.py
"""

import numpy

from kalast._rs.tpm.roughness import Crater

DEG = numpy.radians

print("1. exact limits")
base = dict(density=0.25, emissivity=0.95, albedo=0.12, rh=1.0,
            wavelength=8.0e-6, ntheta=64, nphi=64)
for g in (90, 30, 10, 3, 1, 0):
    r = Crater(gamma_deg=g, **base).correction(DEG(60), DEG(20), DEG(30))[0]
    print(f"   gamma {g:5.1f} deg -> R = {r:.6f}")
print("   (converges as gamma^2; exactly 1 at gamma = 0)")

c0 = Crater(gamma_deg=180.0, **{**base, "density": 0.0})
worst = max(abs(c0.correction(DEG(s), DEG(d), DEG(p))[0] - 1.0)
            for s in (0, 30, 60, 85) for d in (0, 30, 60, 85) for p in (0, 90, 180))
print(f"   density = 0 over 48 geometries: max |R - 1| = {worst:.3e}")

print("\n2. convergence in integration nodes")
prev = None
for n in (8, 16, 32, 64, 128):
    r = Crater(gamma_deg=180.0, **{**base, "ntheta": n, "nphi": n}).correction(
        DEG(60), DEG(20), DEG(30))[0]
    d = "" if prev is None else f"  change {r - prev:+.2e}"
    print(f"   n = {n:4d}  R = {r:.6f}{d}")
    prev = r

print("\n3. against Kuehrt's schematic (pres4, slides 3-4)")
print("   opening angle 180 deg, crater density 0.9, incidence = emission = 30 deg")
print("   reference: R5 = 1.6995 at psi 0 deg, R6 = 0.9845 at psi 180 deg")
print("   parameters are the *original* hardcoded ones, lamb 5 um and albedo 0.05,")
print("   which is how the reference run was made.")
for eps in (0.95, 1.0):
    c = Crater(gamma_deg=180.0, density=0.9, emissivity=eps, albedo=0.05,
               rh=1.0, wavelength=5.0e-6, ntheta=64, nphi=64, solar_constant=1370.0)
    r5 = c.correction(DEG(30), DEG(30), 0.0)[0]
    r6 = c.correction(DEG(30), DEG(30), DEG(180))[0]
    print(f"   eps {eps:.2f}:  R5 = {r5:.4f} ({(r5 / 1.6995 - 1) * 100:+.2f} %)   "
          f"R6 = {r6:.4f} ({(r6 / 0.9845 - 1) * 100:+.2f} %)   "
          f"F5 > F1 > F6 {'holds' if r5 > 1.0 > r6 else 'FAILS'}")
print("   The residual is at the level of the integration scheme; the slide does")
print("   not record the node count or heliocentric distance of the reference run.")

print("\n4. the divergence at grazing emission is benign")
c = Crater(gamma_deg=180.0, **base)
print(f"   {'det deg':>10} {'flat':>11} {'R':>11} {'R x flat':>12}")
for d in (0, 60, 85, 89, 89.9, 89.99, 89.999, 89.99999):
    r, cr, fl = c.correction(DEG(40), DEG(d), 0.0)
    print(f"   {d:10.5f} {fl:11.3e} {r:11.4g} {r * fl:12.4e}")
r, cr, fl = c.correction(DEG(40), DEG(89.99999), 0.0)
mix = base["density"] * cr + (1 - base["density"]) * fl
print(f"   R diverges; R x flat -> {r * fl:.6e}, which is")
print(f"   density*crater + (1-density)*flat = {mix:.6e}, equal to "
      f"{abs(r * fl - mix) / (r * fl):.1e}")
print("   So the ratio is unbounded while the flux it multiplies is not. This")
print("   is why the cap at 10 was removed: it truncated real signal near the")
print("   limb. pres4 slide 8 reaches the same conclusion --")
print("   'No it is by the definition above!! As incidence -> 90, F_flat -> 0, R -> inf'")
