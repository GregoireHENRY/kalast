#!/usr/bin/env python
"""Exact partial shadowing, against a ray trace that converges onto it.

`kalast._rs.shadowing.lit_fractions` computes each facet's lit fraction as an
*area*, by clipping the facet against everything in front of it in the plane
perpendicular to the Sun. `sim.facet_shadow` answers the same question by
testing 4 points, so its answer is one of `{0, 1/4, 1/2, 3/4, 1}`; that costs
0.7 to 40 mmag on a light curve, measured in
`examples/analytical/shadow_quantisation.py`, and is why this exists.

**Testing an exact method against a sampled one needs care.** A fixed
tolerance against a fixed sample count measures the sampler, not the method,
and would fail against correct code as easily as against wrong code. The
honest test is *convergence*: refine the ray trace and the disagreement must
shrink toward zero. If the clipper were wrong, refining the reference would
converge on a different answer and the gap would stop falling.

Measured on a cratered icosphere: max per-facet disagreement 0.2392, 0.1593,
0.1024, 0.0670, 0.0452 at 45, 91, 231, 561 and 1225 samples per facet -- about
`1/n_div`, which is how finely a barycentric lattice can place a shadow edge
inside a facet.

Pure CPU: no GPU, no window.
"""

import sys
import time

import numpy

from kalast._rs import shadowing as sh

failures: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


# --- geometry, kept in step with examples/analytical/shadow_quantisation.py --


def load_obj(path):
    v, f = [], []
    for line in open(path):
        if line.startswith("v "):
            v.append([float(x) for x in line.split()[1:4]])
        elif line.startswith("f "):
            f.append([int(p.split("/")[0]) - 1 for p in line.split()[1:4]])
    return numpy.array(v, dtype=numpy.float64), numpy.array(f, dtype=numpy.int64)


def cratered_sphere(path, n_craters=14, depth=0.55, width=0.22, seed=3):
    v, f = load_obj(path)
    v /= numpy.linalg.norm(v, axis=1)[:, None]
    rng = numpy.random.default_rng(seed)
    c = rng.normal(size=(n_craters, 3))
    c /= numpy.linalg.norm(c, axis=1)[:, None]
    r = numpy.ones(len(v))
    for ci in c:
        r -= depth * numpy.exp(
            -0.5 * (numpy.arccos(numpy.clip(v @ ci, -1.0, 1.0)) / width) ** 2
        )
    return v * r[:, None], f


def facets(v, f):
    a, b, c = v[f[:, 0]], v[f[:, 1]], v[f[:, 2]]
    n = numpy.cross(b - a, c - a)
    area = 0.5 * numpy.linalg.norm(n, axis=1)
    n /= numpy.linalg.norm(n, axis=1)[:, None]
    n[numpy.sum(n * (a + b + c) / 3.0, axis=1) < 0] *= -1.0
    return a, b, c, n, area


def barycentric(n_div):
    return numpy.array(
        [
            (i / n_div, j / n_div, (n_div - i - j) / n_div)
            for i in range(n_div + 1)
            for j in range(n_div + 1 - i)
        ]
    )


def ray_lit_fraction(a, b, c, n, d, bary, eps):
    """Sampled reference: fraction of a facet's points that can see `d`."""
    e1, e2 = b - a, c - a
    pv = numpy.cross(d, e2)
    det = numpy.einsum("ij,ij->i", e1, pv)
    ok = numpy.abs(det) > 1e-12
    inv = numpy.where(ok, 1.0 / numpy.where(ok, det, 1.0), 0.0)

    frac = numpy.zeros(len(a))
    facing = (n @ d) > 1e-9
    idx = numpy.where(facing)[0]
    pts = (
        bary[None, :, 0, None] * a[idx][:, None, :]
        + bary[None, :, 1, None] * b[idx][:, None, :]
        + bary[None, :, 2, None] * c[idx][:, None, :]
    ) + eps * n[idx][:, None, :]
    o = pts.reshape(-1, 3)

    blocked = numpy.zeros(len(o), dtype=bool)
    for s in range(0, len(o), 2048):
        oc = o[s : s + 2048]
        tv = oc[:, None, :] - a[None, :, :]
        u = numpy.einsum("ijk,jk->ij", tv, pv) * inv[None, :]
        qv = numpy.cross(tv, e1[None, :, :])
        vv = (qv @ d) * inv[None, :]
        t = numpy.einsum("ijk,jk->ij", qv, e2) * inv[None, :]
        blocked[s : s + 2048] = (
            ok[None, :] & (u >= 0) & (u <= 1) & (vv >= 0) & (u + vv <= 1) & (t > 1e-9)
        ).any(axis=1)
    frac[idx] = 1.0 - blocked.reshape(len(idx), -1).mean(axis=1)
    return frac


MESH = "res/ico2.obj"
V64, F64 = cratered_sphere(MESH)
A, B, C, N, AREA = facets(V64, F64)
EPS = 1e-4 * numpy.sqrt(AREA.mean())
V32 = numpy.ascontiguousarray(V64, dtype=numpy.float32)
F32 = numpy.ascontiguousarray(F64, dtype=numpy.uint32)
SUN = numpy.array([numpy.cos(3.6), numpy.sin(3.6), 0.0])

exact = numpy.asarray(sh.lit_fractions(V32, F32, SUN.tolist()), dtype=numpy.float64)
facing = (N @ SUN) > 1e-9

# --- 1. the ray trace converges onto the exact answer ---------------------
errs = []
for n_div in (8, 12, 20, 32):
    ref = ray_lit_fraction(A, B, C, N, SUN, barycentric(n_div), EPS)
    errs.append(float(numpy.abs(exact - ref)[facing].max()))

check(
    "test_ray_trace_converges_onto_the_exact_answer",
    all(errs[i] > errs[i + 1] for i in range(len(errs) - 1)),
    "max|diff| " + ", ".join(f"{e:.4f}" for e in errs) + " at 45, 91, 231, 561 samples",
)
# Halving the lattice spacing must roughly halve the gap. If the clipper were
# wrong the reference would converge somewhere else and this would flatten.
check(
    "test_the_gap_falls_like_the_sampling_not_to_a_floor",
    errs[0] / errs[-1] > 2.5,
    f"{errs[0]:.4f} -> {errs[-1]:.4f}, a factor {errs[0] / errs[-1]:.1f} over 4x n_div",
)

# --- 2. it is not quantised, which is the whole point ---------------------
partial = exact[facing & (exact > 1e-9) & (exact < 1 - 1e-9)]
# Not "no value lands on a quarter" -- a first version asserted that and
# failed against correct code, because an exact area may sit on 0.5 by
# symmetry and nothing forbids it. The property that distinguishes the two
# methods is that the values are not *confined* to the lattice: a 4-point
# sampler can only ever produce 3 distinct interior values, and this produces
# nearly as many as there are partially-lit facets.
distinct = len({round(float(x), 9) for x in partial})
check(
    "test_partial_facets_are_not_confined_to_quarters",
    partial.size > 10 and distinct > 0.9 * partial.size,
    f"{partial.size} partially-lit facets take {distinct} distinct values; "
    f"4-point sampling could produce at most 3",
)

# --- 3. sanity: a convex body has nothing to self-shadow ------------------
sphere_v, sphere_f = load_obj("res/ico3.obj")
sphere_v /= numpy.linalg.norm(sphere_v, axis=1)[:, None]
_, _, _, sn, _ = facets(sphere_v, sphere_f)
sph = numpy.asarray(
    sh.lit_fractions(
        numpy.ascontiguousarray(sphere_v, dtype=numpy.float32),
        numpy.ascontiguousarray(sphere_f, dtype=numpy.uint32),
        SUN.tolist(),
    ),
    dtype=numpy.float64,
)
sun_facing = (sn @ SUN) > 0.02
check(
    "test_a_convex_body_shadows_nothing_it_can_see",
    bool(numpy.all(sph[sun_facing] > 0.999)),
    f"min lit fraction over {sun_facing.sum()} sunward facets = {sph[sun_facing].min():.5f}",
)

# --- 4. and it is fast enough to be usable -------------------------------
# The paper this comes from publishes no timings at all, so here are some.
sizes = []
for mesh in ("res/ico2.obj", "res/ico3.obj", "res/ico4.obj"):
    v, f = cratered_sphere(mesh)
    v32 = numpy.ascontiguousarray(v, dtype=numpy.float32)
    f32 = numpy.ascontiguousarray(f, dtype=numpy.uint32)
    t0 = time.perf_counter()
    sh.lit_fractions(v32, f32, SUN.tolist())
    sizes.append((len(f), time.perf_counter() - t0))

check(
    "test_cost_stays_reasonable_with_facet_count",
    sizes[-1][1] < 2.0,
    ", ".join(f"{n} facets {t * 1e3:.0f} ms" for n, t in sizes),
)

sys.exit(1 if failures else 0)
