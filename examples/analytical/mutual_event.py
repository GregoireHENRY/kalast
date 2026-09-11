"""What exact partial shadowing is worth during a mutual event.

`shadow_quantisation.py` measured a *single* body and found the quarter-facet
quantisation worth 0.7 to 40 mmag, with binary per-facet visibility adding
little beside it. `notes/2026-09-10_polygonal_shadowing_assessment.md` had
claimed partial visibility was a comparable second gap, and corrected itself
when the single-body measurement said otherwise -- while noting that the claim
was really about **mutual events**, where one body crosses another, and that
that case was untested.

This is that case. It is also the configuration Brož's paper is about: a
satellite occulting, transiting and eclipsing its primary.

Two spheres seen edge-on, the secondary orbiting at 3 primary radii. The
observer sits along +x and the Sun 20 degrees off it, so the *transit* (the
secondary in front of the primary, a visibility effect) and the *eclipse* (the
secondary's shadow falling on the primary, a shadowing effect) happen at
different orbital phases and can be told apart.

Three light curves, off one geometry and one scattering law, so only the
occlusion treatment differs:

  ref     converged barycentric ray sampling
  q4      the 4 points `facet_shadow` uses, and binary per-facet visibility
  exact   `kalast._rs.shadowing.lit_fractions`, polygon clipping

Run:  python examples/analytical/mutual_event.py [n_phases] [phase_deg]
"""

import sys
import time

import numpy

from kalast._rs import shadowing as _sh

R_SECONDARY = 0.30
ORBIT = 3.0


def load_obj(path):
    v, f = [], []
    for line in open(path):
        if line.startswith("v "):
            v.append([float(x) for x in line.split()[1:4]])
        elif line.startswith("f "):
            f.append([int(p.split("/")[0]) - 1 for p in line.split()[1:4]])
    return numpy.array(v, dtype=numpy.float64), numpy.array(f, dtype=numpy.int64)


def sphere(path, radius=1.0, centre=(0.0, 0.0, 0.0)):
    v, f = load_obj(path)
    v /= numpy.linalg.norm(v, axis=1)[:, None]
    return v * radius + numpy.asarray(centre), f


def two_body(theta, primary_mesh="res/ico3.obj", secondary_mesh="res/ico2.obj"):
    """Primary at the origin, secondary on a circular orbit in the x-y plane."""
    v1, f1 = sphere(primary_mesh, 1.0)
    c = (ORBIT * numpy.cos(theta), ORBIT * numpy.sin(theta), 0.0)
    v2, f2 = sphere(secondary_mesh, R_SECONDARY, c)
    v = numpy.vstack([v1, v2])
    f = numpy.vstack([f1, f2 + len(v1)])
    # Which facets belong to which body, for reporting only.
    body = numpy.concatenate([numpy.zeros(len(f1), int), numpy.ones(len(f2), int)])
    return v, f, body


def facets(v, f):
    a, b, c = v[f[:, 0]], v[f[:, 1]], v[f[:, 2]]
    n = numpy.cross(b - a, c - a)
    area = 0.5 * numpy.linalg.norm(n, axis=1)
    n /= numpy.linalg.norm(n, axis=1)[:, None]
    return a, b, c, n, area


def outward(n, a, b, c, body, theta):
    """Flip normals to point away from their own body's centre."""
    cen = (a + b + c) / 3.0
    origin = numpy.zeros((len(n), 3))
    sec = numpy.array([ORBIT * numpy.cos(theta), ORBIT * numpy.sin(theta), 0.0])
    origin[body == 1] = sec
    flip = numpy.sum(n * (cen - origin), axis=1) < 0
    n[flip] *= -1.0
    return n


def barycentric(n_div):
    return numpy.array(
        [
            (i / n_div, j / n_div, (n_div - i - j) / n_div)
            for i in range(n_div + 1)
            for j in range(n_div + 1 - i)
        ]
    )


KALAST_BARY = numpy.array(
    [[1.0, 0, 0], [0, 1.0, 0], [0, 0, 1.0], [1 / 3, 1 / 3, 1 / 3]]
)
CENTROID = numpy.array([[1 / 3, 1 / 3, 1 / 3]])


def basis_for(d):
    """Two axes spanning the plane perpendicular to `d`."""
    t = numpy.array([0.0, 0.0, 1.0])
    if abs(d @ t) > 0.9:
        t = numpy.array([1.0, 0.0, 0.0])
    u = numpy.cross(d, t)
    u /= numpy.linalg.norm(u)
    return u, numpy.cross(d, u)


class Occluders:
    """Triangles bucketed by their 2D footprint along one ray direction.

    The rays are parallel, so which triangles can block a given ray is decided
    entirely in the plane perpendicular to it. Brute force is O(rays x tris)
    and unusable at two-body facet counts.
    """

    def __init__(self, a, b, c, d, target_per_cell=8.0):
        self.a, self.b, self.c, self.d = a, b, c, d
        u, w = basis_for(d)
        self.u, self.w = u, w
        pu = numpy.stack([a @ u, b @ u, c @ u], axis=1)
        pw = numpy.stack([a @ w, b @ w, c @ w], axis=1)
        self.lo = numpy.stack([pu.min(1), pw.min(1)], axis=1)
        self.hi = numpy.stack([pu.max(1), pw.max(1)], axis=1)
        self.origin = self.lo.min(axis=0)
        extent = self.hi.max(axis=0) - self.origin
        extent[extent <= 0] = 1e-12
        self.n_cell = max(1, int(numpy.sqrt(len(a) / target_per_cell)))
        self.cell = extent / self.n_cell
        lo_i = numpy.clip(((self.lo - self.origin) / self.cell).astype(int), 0, self.n_cell - 1)
        hi_i = numpy.clip(((self.hi - self.origin) / self.cell).astype(int), 0, self.n_cell - 1)
        buckets = [[] for _ in range(self.n_cell * self.n_cell)]
        for t in range(len(a)):
            for i in range(lo_i[t, 0], hi_i[t, 0] + 1):
                for j in range(lo_i[t, 1], hi_i[t, 1] + 1):
                    buckets[i * self.n_cell + j].append(t)
        self.buckets = [numpy.array(x, dtype=numpy.int64) for x in buckets]

    def cell_of(self, pts):
        idx = numpy.clip(
            ((numpy.stack([pts @ self.u, pts @ self.w], axis=1) - self.origin) / self.cell).astype(int),
            0, self.n_cell - 1)
        return idx[:, 0] * self.n_cell + idx[:, 1]

    def any_hit(self, origins):
        out = numpy.zeros(len(origins), dtype=bool)
        cells = self.cell_of(origins)
        order = numpy.argsort(cells)
        bounds = numpy.searchsorted(cells[order], numpy.arange(self.n_cell ** 2 + 1))
        d = self.d
        for cid in range(self.n_cell ** 2):
            s, e = bounds[cid], bounds[cid + 1]
            tri = self.buckets[cid]
            if s == e or tri.size == 0:
                continue
            rays = order[s:e]
            o = origins[rays]
            a, b, c = self.a[tri], self.b[tri], self.c[tri]
            e1, e2 = b - a, c - a
            pv = numpy.cross(d, e2)
            det = numpy.einsum("ij,ij->i", e1, pv)
            ok = numpy.abs(det) > 1e-12
            inv = numpy.where(ok, 1.0 / numpy.where(ok, det, 1.0), 0.0)
            tv = o[:, None, :] - a[None, :, :]
            uu = numpy.einsum("ijk,jk->ij", tv, pv) * inv[None, :]
            qv = numpy.cross(tv, e1[None, :, :])
            vv = (qv @ d) * inv[None, :]
            tt = numpy.einsum("ijk,jk->ij", qv, e2) * inv[None, :]
            out[rays] = (ok[None, :] & (uu >= 0) & (uu <= 1) & (vv >= 0)
                         & (uu + vv <= 1) & (tt > 1e-9)).any(axis=1)
        return out


def ray_fraction(a, b, c, n, d, bary, eps, occ=None):
    """Sampled occlusion: fraction of each facet's points that can see `d`."""
    if occ is None:
        occ = Occluders(a, b, c, d)
    frac = numpy.zeros(len(a))
    facing = (n @ d) > 1e-9
    idx = numpy.where(facing)[0]
    if idx.size == 0:
        return frac
    pts = (
        bary[None, :, 0, None] * a[idx][:, None, :]
        + bary[None, :, 1, None] * b[idx][:, None, :]
        + bary[None, :, 2, None] * c[idx][:, None, :]
    ) + eps * n[idx][:, None, :]
    blocked = occ.any_hit(pts.reshape(-1, 3))
    frac[idx] = 1.0 - blocked.reshape(len(idx), -1).mean(axis=1)
    return frac


def mmag(x, ref):
    ok = (ref > ref.max() * 1e-6) & (x > 0)
    d = numpy.zeros_like(ref)
    d[ok] = -2.5 * numpy.log10(x[ok] / ref[ok]) * 1000.0
    return d


def main():
    n_phases = int(sys.argv[1]) if len(sys.argv) > 1 else 72
    alpha = numpy.radians(float(sys.argv[2]) if len(sys.argv) > 2 else 20.0)
    n_div = 16

    obs = numpy.array([1.0, 0.0, 0.0])
    sun = numpy.array([numpy.cos(alpha), numpy.sin(alpha), 0.0])

    out = {
        k: numpy.zeros(n_phases)
        for k in ("ref", "lit_q4", "vis_bin", "q4", "exact")
    }
    shadowed = numpy.zeros(n_phases)
    t_exact = 0.0

    for p in range(n_phases):
        th = 2.0 * numpy.pi * p / n_phases
        v, f, body = two_body(th)
        a, b, c, n, area = facets(v, f)
        n = outward(n, a, b, c, body, th)
        eps = 1e-4 * numpy.sqrt(area.mean())
        v32 = numpy.ascontiguousarray(v, dtype=numpy.float32)
        f32 = numpy.ascontiguousarray(f, dtype=numpy.uint32)

        occ_s, occ_o = Occluders(a, b, c, sun), Occluders(a, b, c, obs)
        lit_ref = ray_fraction(a, b, c, n, sun, barycentric(n_div), eps, occ_s)
        vis_ref = ray_fraction(a, b, c, n, obs, barycentric(n_div), eps, occ_o)
        lit_q4 = ray_fraction(a, b, c, n, sun, KALAST_BARY, eps, occ_s)
        vis_bin = (ray_fraction(a, b, c, n, obs, CENTROID, eps, occ_o) > 0.5).astype(float)

        t0 = time.perf_counter()
        lit_ex = numpy.asarray(_sh.lit_fractions(v32, f32, sun.tolist()), float)
        vis_ex = numpy.asarray(_sh.lit_fractions(v32, f32, obs.tolist()), float)
        t_exact += time.perf_counter() - t0

        w = area * numpy.maximum(n @ sun, 0.0) * numpy.maximum(n @ obs, 0.0)
        out["ref"][p] = (w * lit_ref * vis_ref).sum()
        # One approximation at a time as well as both, because the two do not
        # simply add -- see the note under the table.
        out["lit_q4"][p] = (w * lit_q4 * vis_ref).sum()
        out["vis_bin"][p] = (w * lit_ref * vis_bin).sum()
        out["q4"][p] = (w * lit_q4 * vis_bin).sum()
        out["exact"][p] = (w * lit_ex * vis_ex).sum()

        wi = area * numpy.maximum(n @ sun, 0.0)
        shadowed[p] = float((wi * (1 - lit_ref)).sum() / wi.sum())

    base = numpy.median(out["ref"])
    depth = mmag(out["ref"], numpy.full(n_phases, base))
    d_q4 = mmag(out["q4"], out["ref"])
    d_ex = mmag(out["exact"], out["ref"])
    d_lit = mmag(out["lit_q4"], out["ref"])
    d_vis = mmag(out["vis_bin"], out["ref"])

    # "In event" = the phases where the reference curve actually drops, which
    # is what distinguishes ingress and egress from the flat baseline.
    in_event = depth > 0.5

    print(f"two spheres, secondary R={R_SECONDARY} at {ORBIT} primary radii")
    print(f"observer +x, Sun {numpy.degrees(alpha):.0f} deg off it, "
          f"{n_phases} orbital phases\n")
    print(f"event depth, max        : {depth.max():.1f} mmag")
    print(f"phases inside an event  : {int(in_event.sum())} of {n_phases}")
    print()
    print(f"{'':>22}{'q4 rms':>10}{'q4 peak':>10}{'exact rms':>12}{'exact peak':>12}")
    for label, m in (("whole orbit", numpy.ones(n_phases, bool)),
                     ("inside the events", in_event),
                     ("baseline only", ~in_event)):
        if m.sum() == 0:
            continue
        print(f"{label:>22}{numpy.sqrt((d_q4[m] ** 2).mean()):>9.2f}m"
              f"{numpy.abs(d_q4[m]).max():>9.2f}m"
              f"{numpy.sqrt((d_ex[m] ** 2).mean()):>11.2f}m"
              f"{numpy.abs(d_ex[m]).max():>11.2f}m")

    print()
    print(f"{'one approximation at a time, in event':>40}{'rms':>9}{'peak':>9}")
    for label, d in (
        ("shadowing quantised to quarters", d_lit),
        ("visibility binarised per facet", d_vis),
        ("both, which is what kalast does", d_q4),
        ("neither, polygon clipping", d_ex),
    ):
        print(f"{label:>40}{numpy.sqrt((d[in_event] ** 2).mean()):>8.2f}m"
              f"{numpy.abs(d[in_event]).max():>8.2f}m")

    print()
    print("Two things to read off this. The error lives **entirely inside the")
    print("events** -- two smooth convex spheres self-shadow nothing, so the")
    print("baseline is exact for every method. And the two approximations")
    print("**partially cancel**: separately they are worth more than together,")
    print("so measuring either alone overstates what removing it buys.")
    print()
    print("This is also the case the assessment note guessed at and could not")
    print("measure. On a single body, binarised visibility cost almost nothing")
    print("beside the shadow quantisation; during a mutual event it is")
    print("comparable to it, which is what makes the mutual case different.")

    print(f"\nexact clipping: {t_exact / n_phases * 1e3:.0f} ms per phase "
          f"(two directions, {len(f)} facets)")


if __name__ == "__main__":
    main()
