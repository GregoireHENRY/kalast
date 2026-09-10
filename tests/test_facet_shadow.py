#!/usr/bin/env python
"""The shadow the thermophysical model runs on, pinned two ways.

`shaders/facet_shadow.wgsl` is not the render's shadow term, and the two are
easy to confuse -- the shader's own header claimed they "cannot disagree"
until this test was written, which is false the moment `shadow_pcf > 0`. The
render filters over a `(2N+1)^2` kernel and widens its normal offset by
`(1 + N)` to match that kernel's reach; the compute path takes one tap and
returns a binary occlusion.

That divergence is deliberate. The Sun is a point source here, so occlusion
is binary, and PCF is image-space antialiasing with nothing physical to
contribute to a boundary condition. But "deliberate" was only ever a property
of the code, never asserted anywhere, and the bias constants it depends on
were fitted by a one-off sweep that lived in a note
(`notes/2026-09-08_shadow_bias.md`: slope 1, floor 1, offset sqrt2).

So two things are checked here:

1. **Invariance to `shadow_pcf`.** If someone adds a kernel to the compute
   path, or scales its normal offset the way the fragment shader does, the
   physics silently changes under every existing result. This catches that.
2. **Agreement with ray tracing.** The shadow map is a sampled approximation;
   a ray from a facet centroid to the Sun is not. This re-runs, in miniature,
   the sweep that fitted the bias -- so a regression in the offset, the bias
   or the projection shows up as facets the map calls lit that the ray says
   are blocked.

Opens a real window: there is no way to exercise a compute pass against a
shadow map without one. One `App` only -- constructing a second in the same
process panics with `RecreationAttempt`. `shadow_pcf` is live, so the sweep
changes it on the one app rather than needing three.
"""

import sys

import numpy

from kalast.app import App

MESH = "res/plane_crater_1024-5000_h=0.437.obj"
SUN = [0.0, 20.0, 5.0]

# Sun angles are near-grazing on the far wall, which is where the bias earns
# its keep. Only facets actually turned toward the Sun are compared: a facet
# facing away is dark because of its own orientation, which is insolation's
# question and not the shadow map's, and the map is not fitted for it.
SUNWARD_MIN = 0.05

# Ray tracing starts this far along the normal so a facet does not occlude
# itself. A crater 1 unit across with 2048 facets has edges ~0.03 long, so
# this is well inside a facet and well outside floating-point noise.
RAY_EPS = 1e-4

# Sun angles, radians about +X, matching the crater example's own sweep:
# `sun = [0, 20 sin a, 20 cos a]`. Fifteen of them covering full day, both
# terminators and the night side -- the same coverage
# `notes/2026-09-08_shadow_bias.md` used to fit the bias, and for the same
# reason. One favourable angle is a weak test: with the Sun high, a 100x error
# in the bias moved false-lit only 18 -> 24 out of 1540, which no sane budget
# would catch. The bias earns its keep at grazing incidence, where a shadow
# texel spans a large depth range, so the terminator and night angles are
# where a regression actually shows.
N_ANGLES = 15

# Budgets, calibrated by measuring both a healthy build and a deliberately
# broken one (bias x100 in `facet_shadow.wgsl`) rather than by picking round
# numbers:
#
# |                       | healthy | bias x100 |
# |-----------------------|---------|-----------|
# | false-lit, aggregate  |  0.33 % |    0.76 % |
# | false-lit, worst angle|  1.39 % |   14.29 % |
# | false-dark            |  0.35 % |    0.35 % |
#
# **The worst single angle is the detector**, not the aggregate. Averaging
# over the sweep dilutes a grazing-incidence failure into 15 angles of good
# behaviour: the 100x error moves the aggregate by 2.3x but the worst angle by
# 10x. The first version of this test asserted only on the aggregate and
# passed against the broken shader, which is how this was found.
#
# The aggregate is kept as a loose sanity bound. False-dark is unmoved by a
# bias error -- too much bias lets facets escape the depth test, which can
# only ever manufacture false *lit* -- so it is bounded near its measured
# value to catch the opposite kind of regression.
FALSE_LIT_WORST_ANGLE_MAX = 0.05
FALSE_LIT_MAX = 0.01
FALSE_DARK_MAX = 0.01

failures: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def settle(app, sim, body: int, steps: int = 8):
    """Step until `facet_shadow` reports, then a few more.

    The query is requested one frame and read the next, and the first frames
    after a config change still carry the old shadow map, so the last value
    of a short run is the one that reflects the current settings.
    """
    out = None
    for _ in range(steps):
        if not app.running:
            break
        sim.request_facet_shadow(body)
        app.step()
        v = sim.facet_shadow(body)
        if v is not None:
            out = numpy.asarray(v, dtype=numpy.float64).copy()
    return out


def ray_traced_occlusion(mesh, sun) -> tuple[numpy.ndarray, numpy.ndarray]:
    """Ground truth: is the straight line from each facet to the Sun blocked?

    Returns `(occluded, sunward)`, both boolean and one entry per facet.
    """
    facets = mesh.facets
    n = len(facets)
    occluded = numpy.zeros(n, dtype=bool)
    sunward = numpy.zeros(n, dtype=bool)

    sun = numpy.asarray(sun, dtype=numpy.float64)
    for i in range(n):
        f = facets[i]
        p = numpy.asarray(f.pos, dtype=numpy.float64)
        nrm = numpy.asarray(f.normal, dtype=numpy.float64)

        to_sun = sun - p
        dist = numpy.linalg.norm(to_sun)
        u = to_sun / dist

        cos_i = float(nrm @ u)
        sunward[i] = cos_i > SUNWARD_MIN
        if not sunward[i]:
            continue

        hit = mesh.intersect(p + nrm * RAY_EPS, u, False)
        # A hit beyond the Sun is not an occluder. The Sun is 20 units out and
        # the crater is ~1 across, so this only guards the degenerate case.
        if hit is not None:
            _, point = hit
            if numpy.linalg.norm(numpy.asarray(point, dtype=numpy.float64) - p) < dist:
                occluded[i] = True
    return occluded, sunward


def main() -> int:
    app = App()
    c = app.simulation.config
    c.vsync = False
    c.render_back_face = True
    c.access_shadow_map = True
    c.shadow_pcf = 0

    sim = app.simulation
    sim.sun.pos = SUN
    sim.load_mesh(path=MESH, mat=numpy.eye(4), flatten=True)

    base = settle(app, sim, 0)
    if base is None:
        print("FAIL could not read facet_shadow at all -- no GPU?")
        return 1

    # 1. Invariance to shadow_pcf.
    for pcf in (4, 8):
        c.shadow_pcf = pcf
        got = settle(app, sim, 0)
        same = got is not None and numpy.array_equal(base, got)
        differing = int((base != got).sum()) if got is not None else -1
        check(
            f"test_shadow_pcf_{pcf}_does_not_move_the_physics",
            same,
            f"{differing}/{base.size} facets differ; the compute path must not filter",
        )
    c.shadow_pcf = 0

    # 2. Agreement with ray tracing, swept over Sun angle.
    mesh = sim.bodies[0].mesh
    total_cmp = 0
    total_false_lit = 0
    total_false_dark = 0
    worst = (0.0, 0.0)
    shadowed_seen = False

    for j in range(N_ANGLES):
        a = 2.0 * numpy.pi * j / N_ANGLES
        sun = [0.0, 20.0 * float(numpy.sin(a)), 20.0 * float(numpy.cos(a))]
        sim.sun.pos = sun
        got = settle(app, sim, 0)
        if got is None:
            continue

        truth, sunward = ray_traced_occlusion(mesh, sun)
        n_cmp = int(sunward.sum())
        if n_cmp == 0:
            # The Sun straight under a closed crater lights nothing; no facet
            # to compare is a valid outcome, not a failed angle.
            continue

        mapped = got[sunward] > 0.5
        ref = truth[sunward]
        fl = int((~mapped & ref).sum())
        fd = int((mapped & ~ref).sum())

        total_cmp += n_cmp
        total_false_lit += fl
        total_false_dark += fd
        worst = max(worst, (fl / n_cmp, fd / n_cmp))
        if 0.01 < float(mapped.mean()) < 0.99:
            shadowed_seen = True

    check(
        "test_ray_trace_has_facets_to_compare",
        total_cmp > 1000,
        f"{total_cmp} sunward facets over {N_ANGLES} Sun angles",
    )
    if total_cmp == 0:
        return 1

    fl_rate = total_false_lit / total_cmp
    fd_rate = total_false_dark / total_cmp

    check(
        "test_map_does_not_call_blocked_facets_lit_at_any_angle",
        worst[0] <= FALSE_LIT_WORST_ANGLE_MAX,
        f"worst angle {worst[0]:.2%} (budget {FALSE_LIT_WORST_ANGLE_MAX:.0%})",
    )
    check(
        "test_map_does_not_call_blocked_facets_lit",
        fl_rate <= FALSE_LIT_MAX,
        f"{total_false_lit}/{total_cmp} = {fl_rate:.2%} (budget {FALSE_LIT_MAX:.0%})",
    )
    check(
        "test_map_does_not_call_reachable_facets_dark",
        fd_rate <= FALSE_DARK_MAX,
        f"{total_false_dark}/{total_cmp} = {fd_rate:.2%} (budget {FALSE_DARK_MAX:.0%})",
    )

    # If the map reported everything lit, both budgets above would pass on an
    # empty answer. Some angle in a sweep across a deep crater must produce
    # real shadow.
    check(
        "test_the_map_actually_reports_shadow",
        shadowed_seen,
        "at least one Sun angle gave a partly-shadowed body"
        if shadowed_seen
        else "no Sun angle produced any shadow at all",
    )

    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
