#!/usr/bin/env python
"""Hera AFC view of Didymos/Dimorphos, with the renderer left on automatic.

Companion to afc_eclip_didy_manual.py, which pins the same settings by hand.
Here the camera/light frustums and the shadow bias are all fitted per frame
(they default to None = automatic), so the scene needs no tuning as the
bodies move.

Also demonstrates the two frame callbacks: `before_render` sets the scene,
`after_render` consumes GPU results for that same frame -- here, how many
facets of Didymos the shadow map says are shadowed, which over this sweep
picks out the Dimorphos eclipses.
"""

import numpy
import spiceypy as spice

import kalast

from kalast.util import AU_KM, RPD


def before_render(sim: kalast.app.simulation.Simulation, dt: float):
    if sim.state.is_paused:
        return

    et = et0 + sim.state.iteration * simu_dt

    if et > etf:
        return

    sim.export_once()

    (p_sun, _lt) = spice.spkpos("SUN", et, "ECLIPJ2000", "none", "DIDYMOS")
    (p_dimo, _lt) = spice.spkpos("DIMORPHOS", et, "ECLIPJ2000", "none", "DIDYMOS")
    (p_hera, _lt) = spice.spkpos("HERA", et, "ECLIPJ2000", "none", "DIDYMOS")
    m_didy_ej2k = spice.pxform("DIDYMOS_FIXED", "ECLIPJ2000", et)
    m_dimo_ej2k = spice.pxform("DIMORPHOS_FIXED", "ECLIPJ2000", et)
    m_afc_ej2k = spice.pxform("HERA_AFC-1", "ECLIPJ2000", et)

    sim.sun.pos = p_sun / AU_KM * 10.0
    sim.sun.look_anchor()
    sim.bodies[0].mat[:3, :3] = m_didy_ej2k
    sim.bodies[1].mat[:3, 3] = p_dimo
    sim.bodies[1].mat[:3, :3] = m_dimo_ej2k

    sim.camera.pos = p_hera
    sim.camera.dir = m_afc_ej2k @ numpy.array([0.0, 0.0, 1.0])
    sim.camera.up = m_afc_ej2k @ numpy.array([1.0, 0.0, 0.0])

    # Serviced after this frame renders, so `after_render` below reads the
    # answer for the geometry just set -- not the previous frame's.
    sim.request_facet_shadow(0)


def after_render(sim: kalast.app.simulation.Simulation, dt: float):
    frac = sim.facet_shadow()
    if frac is None:
        return

    et = et0 + sim.state.iteration * simu_dt
    if et > etf:
        return

    # Facets the shadow map reports as at least partly occluded from the Sun.
    # Roughly half is simply the night side; a rise above that baseline is
    # Dimorphos eclipsing the primary. `1.0 - frac` is the lit fraction the
    # thermophysical boundary condition wants.
    shadowed = int((frac > 0.0).sum())

    if sim.state.iteration % 200 == 0:
        print(
            f"it={sim.state.iteration:6d} "
            f"{spice.et2utc(et, 'C', 0)}  shadowed facets={shadowed:,}",
            flush=True,
        )


app = kalast.app.App()
app.config.width = 1020
app.config.height = 1020
app.config.color_mode = 0
# Leave vsync on for interactive viewing; set False for any timing work,
# otherwise the loop just reports the display refresh rate.
# app.config.vsync = False
app.simulation.camera.projection.fovy = 5.5 * RPD

spice.kclear()
spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/mk/hera_plan_local.tm")
et0 = spice.str2et("2026-11-05 00:00:00 UTC")
et = et0
simu_dt = 15.0 * 60.0
# Hera's own kernels run out before 2027-05-01, so stop just short of it.
etf = spice.str2et("2027-04-30 00:00:00 UTC")
(p_sun, _lt) = spice.spkpos("SUN", et, "ECLIPJ2000", "none", "DIDYMOS")
(p_dimo, _lt) = spice.spkpos("DIMORPHOS", et, "ECLIPJ2000", "none", "DIDYMOS")
m_didy_ej2k = spice.pxform("DIDYMOS_FIXED", "ECLIPJ2000", et)
m_dimo_ej2k = spice.pxform("DIMORPHOS_FIXED", "ECLIPJ2000", et)

# No `shadow_path` proxy here, deliberately: a coarser occluder is ~1.5x
# faster (see notes/shadow_mesh_comparison/) but the shadow map is what
# `request_facet_shadow` reads, so the facet counts above would then describe
# the proxy's shadow rather than the real one. Add it for pure-render runs.
mat = numpy.eye(4)
mat[:3, :3] = m_didy_ej2k
app.simulation.load_mesh(
    path="/Users/gregoireh/data/spice/hera/kernels/dsk/g_01165mm_spc_obj_didy_0000n00000_v003.obj",
    mat=mat,
    flatten=True,
)

mat = numpy.eye(4)
mat[:3, 3] = p_dimo
mat[:3, :3] = m_dimo_ej2k
app.simulation.load_mesh(
    path="/Users/gregoireh/data/spice/hera/kernels/dsk/g_00243mm_spc_obj_dimo_0000n00000_v004.obj",
    mat=mat,
    flatten=True,
)

app.before_render = before_render
app.after_render = after_render
app.start()
spice.kclear()
