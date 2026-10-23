#!/usr/bin/env python
"""The flux of the rendered frame against `kalast.lightcurve.flux`.

`config.photometry.enabled` draws the bodies a second time each frame with
the main pass's shader built for photometry -- the frame's own shadows, the
same scattering law, each facet's own normal -- and sums every pixel's I/F
times its area: the disc integral the CPU computes exactly. The two must
agree, to what a pixel decides at a limb or a shadow's edge:

1. A rough Hapke sphere, smooth-shaded in the image, at three phase angles:
   the rendered flux within 2e-3 of the CPU's, its normal the facet's.
2. A moon in front of its primary, the Sun behind the camera's shoulder: its
   shadow and its outline on the primary, with the shadow maps and with
   rays where the GPU has them, within 2e-3 of the CPU's.
3. A frame is tagged with the iteration it was drawn at, one flux an
   iteration, none while paused.
4. A body the frame cuts is flagged `clipped`; a perspective camera's frames
   are not measured.

A background window; `res/ico5.obj`, no data paths.
"""

import sys

import numpy

import kalast.mesh
from kalast.app import App
from kalast.lightcurve import flux
from kalast.scattering import Hapke
from kalast.util import RPD

failures: list[str] = []
LAW = Hapke(w=0.126, b=0.36, c=1.0, b0=0.97, h=0.020, theta_bar=18.0 * RPD, k=1.0)


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        print(f"ok   {name}" + (f"  ({detail})" if detail else ""))
    else:
        print(f"FAIL {name}  {detail}")
        failures.append(name)


def unit(v):
    v = numpy.asarray(v, dtype=float)
    return v / numpy.linalg.norm(v)


def rendered(app, steps=8):
    """Step until this frame's flux is back; the last one."""
    sim = app.simulation
    got = []
    for _ in range(steps):
        app.step()
        got += sim.fluxes()
        if got and sim.fluxes_pending == 0:
            break
    return got


def main() -> int:
    ico = kalast.mesh.Mesh(path="res/ico5.obj")
    app = App()
    app.config.open_in_background = True
    sim = app.simulation
    sim.config.huds = []
    sim.config.photometry.enabled = True
    sim.load_mesh(path="res/ico5.obj", smooth=True)
    sim.load_mesh(path="res/ico5.obj", smooth=True)
    for body in sim.bodies:
        body.scattering = LAW
    sim.camera.projection.set_orthographic()
    sim.camera.projection.side = 2.0
    to_site = unit([0.0, -1.0, 0.0])
    sim.camera.pos = (to_site * 30.0).tolist()
    sim.camera.dir = (-to_site).tolist()
    sim.camera.up = [0.0, 0.0, 1.0]

    def pose(i, scale, place):
        sim.bodies[i].mat[:3, :3] = numpy.diag([scale] * 3)
        sim.bodies[i].mat[:3, 3] = place

    # 1. A sphere alone, the moon far behind it and out of the frame's light.
    pose(0, 1.0, [0.0, 0.0, 0.0])
    pose(1, 0.25, [0.0, 50.0, 0.0])
    for degrees in (5.0, 40.0, 75.0):
        a = numpy.radians(degrees)
        to_sun = unit([numpy.sin(a), -numpy.cos(a), 0.0])
        sim.sun.pos = (to_sun * 1.0e8).tolist()
        got = rendered(app)
        cpu = flux(ico.positions, ico.indices, to_sun, to_site, LAW).flux
        gpu = got[-1][1] if got else float("nan")
        check(f"a sphere at {degrees:.0f} deg of phase: the rendered flux is the CPU's", abs(gpu / cpu - 1) < 2e-3, f"{gpu / cpu - 1:+.2e}")

    # 2. A moon before its primary, its shadow on it.
    to_sun = unit([0.25, -1.0, 0.1])
    sim.sun.pos = (to_sun * 1.0e8).tolist()
    place = numpy.array([0.35, -1.6, 0.05])
    pose(1, 0.25, place)
    cpu = flux([ico.positions, ico.positions * 0.25 + place], [ico.indices, ico.indices], to_sun, to_site, LAW).flux
    apart = (flux(ico.positions, ico.indices, to_sun, to_site, LAW).flux
             + flux(ico.positions * 0.25 + place, ico.indices, to_sun, to_site, LAW).flux)
    check("the moon's event takes light", cpu < apart * 0.99, f"{-2.5 * numpy.log10(cpu / apart) * 1000:.1f} mmag")
    for method in ("maps", "rays"):
        sim.config.shadows.method = method
        got = rendered(app, 12)
        if method == "rays" and not sim.rays:
            print("skip rays: not on this GPU")
            continue
        gpu = got[-1][1] if got else float("nan")
        check(f"a moon's shadow and outline ({method}): the rendered flux is the CPU's", abs(gpu / cpu - 1) < 2e-3, f"{gpu / cpu - 1:+.2e}")
    sim.config.shadows.method = "maps"

    # 3. One flux for each iteration that advanced, tagged with it; none
    # while paused.
    # From a pause, nothing in flight: the counter holds still there, where
    # between two steps of a run it can be a frame or two ahead of the
    # frames drawn.
    sim.state.toggle_pause()
    for _ in range(12):
        app.step()
    sim.fluxes()
    start = sim.state.iteration
    sim.state.toggle_pause()
    for _ in range(5):
        app.step()
    sim.state.toggle_pause()
    got = []
    for _ in range(12):
        app.step()
        got += sim.fluxes()
    end = sim.state.iteration
    iterations = [g[0] for g in got]
    check("each iteration that advanced has its flux, once", iterations == list(range(start, end)), f"{iterations} for {start}..{end}")
    for _ in range(5):
        app.step()
    check("none while paused", sim.fluxes() == [] and sim.state.iteration == end)
    sim.state.toggle_pause()

    # 4. Cut by the frame, and a perspective camera.
    sim.camera.projection.side = 0.5
    got = rendered(app)
    check("a body the frame cuts is flagged", bool(got) and got[-1][2], str(got[-1:]))
    sim.camera.projection.side = 2.0
    got = rendered(app)
    check("whole again, it is not", bool(got) and not got[-1][2])
    sim.camera.projection.set_perspective()
    for _ in range(4):
        app.step()
    sim.fluxes()
    for _ in range(4):
        app.step()
    check("a perspective camera's frames are not measured", sim.fluxes() == [])

    app.close()
    app.step()
    print(f"\n{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
