#!/usr/bin/env python
"""A body's own shadows from its horizon map, held to rays.

A sphere of radius 100 carries hills and craters with rims, 1.5 to 3.5 high
and 4 to 9 wide, about its +x point, as an icosphere of 81,920 facets. The
Sun stands low over them, 1.5 to 12 degrees above the horizon there, from
three directions. For each facet turned toward it near the terrain, a ray
from the facet's centre to the Sun against the mesh is the truth; the
per-facet shadow query (`sim.facet_shadow`) answers from the shadow map, and
again with `body.horizon_map = True`, from the horizon map, the body no
longer drawn into its own layer.

The horizon map's misses are its 32 azimuths -- a hill narrower than the
11 degrees between two of them, seen between them -- and the shadow map's
its bias, which lifts a lookup off the surface and lights what a ray says is
dark. Measured when this was written, over the twelve Suns:

| | facets wrongly lit | wrongly dark |
|---|---|---|
| shadow map | 2.00 % | 0.00 % |
| horizon map | 0.65 % | 0.03 % |

Then the Sun a disc, its angular radius 0.005 (near the Sun's from Mars), 4
degrees high: what each facet in a penumbra sees of it, from the image --
with the horizon map, one value a facet, its centre's; with the shadow map's
walk, its pixels' mean -- against rays to 64 points of the limb-darkened
disc, over 200 of the facets either calls a penumbra. Measured: the horizon
map 70 % of them within 0.05 of the rays, the mean error +0.04; the walk 40 %
and +0.08. And turned off again, the shadow map's answer is back.

Opens a window, in the background.
"""

import glob
import math
import os
import sys
import tempfile

import numpy
from _png import read_png

from kalast.app import App

failures: list[str] = []

R = 100.0
LEVEL = 6
FALSE_LIT_MAX = 0.015
FALSE_LIT_WORST_MAX = 0.03
FALSE_DARK_MAX = 0.005
SUNS = [(e, az) for e in (1.5, 3.0, 6.0, 12.0) for az in (90.0, 30.0, 200.0)]
DISC = 0.005
LIMB_DARKENING = 0.56
WITHIN = 0.05
WITHIN_SHARE_MIN = 0.60


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def icosphere(level: int) -> tuple[numpy.ndarray, numpy.ndarray]:
    g = (1 + 5**0.5) / 2
    v = [(-1, g, 0), (1, g, 0), (-1, -g, 0), (1, -g, 0), (0, -1, g), (0, 1, g),
         (0, -1, -g), (0, 1, -g), (g, 0, -1), (g, 0, 1), (-g, 0, -1), (-g, 0, 1)]
    v = [numpy.array(p, float) / numpy.linalg.norm(p) for p in v]
    f = [(0, 11, 5), (0, 5, 1), (0, 1, 7), (0, 7, 10), (0, 10, 11), (1, 5, 9), (5, 11, 4), (11, 10, 2),
         (10, 7, 6), (7, 1, 8), (3, 9, 4), (3, 4, 2), (3, 2, 6), (3, 6, 8), (3, 8, 9), (4, 9, 5),
         (2, 4, 11), (6, 2, 10), (8, 6, 7), (9, 8, 1)]
    for _ in range(level):
        mid, nf = {}, []

        def m(a, b):
            k = (min(a, b), max(a, b))
            if k not in mid:
                p = v[a] + v[b]
                v.append(p / numpy.linalg.norm(p))
                mid[k] = len(v) - 1
            return mid[k]

        for a, b, c in f:
            ab, bc, ca = m(a, b), m(b, c), m(c, a)
            nf += [(a, ab, ca), (ab, b, bc), (ca, bc, c), (ab, bc, ca)]
        f = nf
    return numpy.array(v), numpy.array(f)


def terrain() -> list[tuple[str, numpy.ndarray, float, float]]:
    rng = numpy.random.default_rng(7)
    out = []
    for k in range(14):
        lat, lon = numpy.radians(rng.uniform(-18, 18)), numpy.radians(rng.uniform(-18, 18))
        c = numpy.array([math.cos(lat) * math.cos(lon), math.cos(lat) * math.sin(lon), math.sin(lat)])
        out.append(("crater" if k % 2 else "hill", c, rng.uniform(4, 9) / R, rng.uniform(1.5, 3.5)))
    return out


def radius(d: numpy.ndarray, features) -> numpy.ndarray:
    r = numpy.full(len(d), R)
    for kind, c, w, h in features:
        a = numpy.arccos(numpy.clip(d @ c, -1, 1))
        if kind == "hill":
            r += h * numpy.exp(-((a / w) ** 2))
        else:
            r += -h * numpy.exp(-((a / (0.7 * w)) ** 2)) + 0.5 * h * numpy.exp(-(((a - w) / (0.3 * w)) ** 2))
    return r


def sun_direction(elevation: float, azimuth: float) -> numpy.ndarray:
    """Over +x: up +x, east +y, north +z."""
    e, a = math.radians(elevation), math.radians(azimuth)
    return math.sin(e) * numpy.array([1.0, 0, 0]) + math.cos(e) * (
        math.sin(a) * numpy.array([0, 1.0, 0]) + math.cos(a) * numpy.array([0, 0, 1.0])
    )


def main() -> int:
    v, f = icosphere(LEVEL)
    pos = v * radius(v, terrain())[:, None]
    tmp = tempfile.mkdtemp(prefix="kalast_horizon_")
    obj = os.path.join(tmp, "bumpy.obj")
    with open(obj, "w") as fh:
        fh.write("".join("v %.7f %.7f %.7f\n" % tuple(p) for p in pos))
        fh.write("".join("f %d %d %d\n" % (a + 1, b + 1, c + 1) for a, b, c in f))
    centre = pos[f].mean(axis=1)
    normal = numpy.cross(pos[f[:, 1]] - pos[f[:, 0]], pos[f[:, 2]] - pos[f[:, 0]])
    normal /= numpy.linalg.norm(normal, axis=1)[:, None]
    near = (centre / numpy.linalg.norm(centre, axis=1)[:, None])[:, 0] > math.cos(math.radians(24))

    app = App()
    app.config.open_in_background = True
    c = app.simulation.config
    out = os.path.join(tmp, "frames")
    c.export.dir, c.export.hud, c.export.sync = out, False, True
    c.shadows.access_shadow_map = True
    c.shadows.pcf = 0
    sim = app.simulation
    sim.load_mesh(path=obj)
    body = sim.bodies[0]
    mesh = body.mesh

    def facet_shadow() -> numpy.ndarray:
        got = None
        for _ in range(8):
            sim.request_facet_shadow(0)
            app.step()
            v = sim.facet_shadow(0)
            if v is not None:
                got = numpy.asarray(v, float).copy()
        return got

    # The truth for each Sun: a ray from each facet turned toward it near
    # the terrain.
    rng = numpy.random.default_rng(1)
    truths = []
    for e, az in SUNS:
        u = sun_direction(e, az)
        idx = numpy.nonzero((normal @ u > 0.05) & near)[0]
        idx = idx if len(idx) <= 2000 else rng.choice(idx, 2000, replace=False)
        dark = numpy.array([mesh.intersect(centre[i] + normal[i] * 1e-4, u, False) is not None for i in idx])
        truths.append((u, idx, dark))

    rates = {}
    answers = {}
    for mode in ("map", "horizon"):
        body.horizon_map = mode == "horizon"
        lit_wrong = dark_wrong = compared = 0
        worst = 0.0
        for (u, idx, dark), sun in zip(truths, SUNS):
            sim.sun.pos = (1e7 * u).tolist()
            got = facet_shadow()
            answers[(mode, sun)] = got
            said = got[idx] > 0.5
            lit_wrong += int((~said & dark).sum())
            dark_wrong += int((said & ~dark).sum())
            compared += len(idx)
            worst = max(worst, (~said & dark).sum() / len(idx))
        rates[mode] = (lit_wrong / compared, dark_wrong / compared, worst, compared)
        print("     %-7s %d facets: %.2f %% wrongly lit (worst Sun %.2f %%), %.2f %% wrongly dark" % (
            mode, compared, 100 * rates[mode][0], 100 * worst, 100 * rates[mode][1]))

    lit, dark, worst, _ = rates["horizon"]
    check("the horizon map lights few facets a ray says are dark", lit <= FALSE_LIT_MAX, f"{100 * lit:.2f} %")
    check("at every Sun", worst <= FALSE_LIT_WORST_MAX, f"worst {100 * worst:.2f} %")
    check("and darkens few it says are lit", dark <= FALSE_DARK_MAX, f"{100 * dark:.2f} %")
    check("no worse than the shadow map", lit + dark <= rates["map"][0] + rates["map"][1],
          f"{100 * (lit + dark):.2f} % wrong against {100 * (rates['map'][0] + rates['map'][1]):.2f} %")

    # The Sun a disc: the image, a facet's light over its Lambert cos i.
    c.image.width = c.image.height = 900
    c.axes.style = "off"
    c.wireframe.mode = 0
    c.shading.msaa = 1
    c.shading.srgb_mode = 1
    c.shading.lod = False
    exposure = 0.8
    c.light.exposure = exposure
    c.light.sun_as_point = False
    c.light.sun_radius = DISC * 1e7
    u = sun_direction(4.0, 60.0)
    sim.sun.pos = (1e7 * u).tolist()
    cam = sim.camera
    cam.pos, cam.dir, cam.up = [330.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]
    cam.projection.fovy = math.radians(50)
    def disc_seen() -> numpy.ndarray:
        """Per facet, its pixels' light over its Lambert cos i."""
        for _ in range(4):
            app.step()
        before = set(glob.glob(out + "/*.png"))
        sim.export_once()
        sim.request_facet_id()
        app.step()
        ids, offsets = sim.facet_id_map()
        app.step()
        new = set(glob.glob(out + "/*.png")) - before
        image = read_png(new.pop())[..., 0].astype(float)
        drawn = ids > 0
        facet = ids[drawn].astype(int) - 1 - offsets[0]
        cos_i = normal[facet] @ u
        ok = cos_i > 0.05
        total = numpy.bincount(facet[ok], image[drawn][ok] / (255.0 * exposure * cos_i[ok]), minlength=len(f))
        count = numpy.bincount(facet[ok], minlength=len(f))
        return numpy.where(count > 0, total / numpy.maximum(count, 1), numpy.nan)

    seen = {}
    for mode in ("horizon", "map"):
        body.horizon_map = mode == "horizon"
        seen[mode] = disc_seen()
    both = numpy.isfinite(seen["horizon"]) & numpy.isfinite(seen["map"])
    partial = lambda x: (x > 0.03) & (x < 0.97)
    penumbra = numpy.nonzero(both & (partial(seen["horizon"]) | partial(seen["map"])))[0]
    penumbra = penumbra if len(penumbra) <= 200 else rng.choice(penumbra, 200, replace=False)
    k = numpy.arange(64)
    r = numpy.sqrt((k + 0.5) / 64)
    t = k * 2.39996323
    points = numpy.stack([r * numpy.cos(t), r * numpy.sin(t)], 1)
    weight = 1 - LIMB_DARKENING * (1 - numpy.sqrt(1 - (points**2).sum(1)))
    e1 = numpy.cross(u, [0, 0, 1.0])
    e1 /= numpy.linalg.norm(e1)
    e2 = numpy.cross(u, e1)
    rays = u[None] + DISC * (points[:, :1] * e1 + points[:, 1:] * e2)
    rays /= numpy.linalg.norm(rays, axis=1)[:, None]
    truth = numpy.zeros(len(penumbra))
    for j, i in enumerate(penumbra):
        p = centre[i] + normal[i] * 1e-4
        truth[j] = sum(w for d, w in zip(rays, weight) if normal[i] @ d > 0 and mesh.intersect(p, d, False) is None) / weight.sum()
    share, mean = {}, {}
    for mode in ("horizon", "map"):
        error = seen[mode][penumbra] - truth
        share[mode], mean[mode] = float((abs(error) < WITHIN).mean()), float(error.mean())
        print("     %-7s %d facets in a penumbra: %.0f %% within %.2f of the rays, mean error %+.3f" % (
            mode, len(penumbra), 100 * share[mode], WITHIN, mean[mode]))
    check("the Sun a disc: a facet sees what the rays do, as well as the walk does",
          len(penumbra) > 50 and share["horizon"] >= max(WITHIN_SHARE_MIN, share["map"]),
          f"{100 * share['horizon']:.0f} % within {WITHIN}, the walk {100 * share['map']:.0f} %")
    check("and not more or less on the whole", abs(mean["horizon"]) <= 0.05, f"mean {mean['horizon']:+.3f}")

    # Off again: the shadow map's answer back.
    c.light.sun_as_point = True
    body.horizon_map = False
    sim.sun.pos = (1e7 * truths[0][0]).tolist()
    got = facet_shadow()
    check("turned off, the shadow map's answer is back", numpy.array_equal(got, answers[("map", SUNS[0])]),
          f"{int((got != answers[('map', SUNS[0])]).sum())} facets differ")

    app.close()
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
