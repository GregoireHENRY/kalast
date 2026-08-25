#!/usr/bin/env python
"""Same render as afc_eclip_didy_manual.py, written the way kalast expects now.

Kept alongside the original on purpose: diff the two to see what the automatic
frustum and shadow fitting removed.

    git diff --no-index afc_eclip_didy_manual.py afc_eclip_didy_auto.py

Six lines of hand-tuned, scene-specific numbers are gone. They were correct
only for a 780 m body seen from 25 km, and had to be re-derived by hand for
any other scene. Rendered output is the same to 397 pixels in 1,040,400, and
the fitted light frustum is strictly better than the one it replaces --
21.8x the shadow depth resolution. See notes/renderer_auto_fit_wireframe/.

Paths below are absolute and machine-specific, matching every other example in
this repo. Point them at your own copies of the kernels and shape models.
"""

import numpy
import spiceypy as spice

import kalast

from kalast.util import AU_KM, RPD


def tick(sim: kalast.app.simulation.Simulation, dt: float):
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

    # The camera is driven entirely from SPICE: position from the Hera SPK,
    # orientation from the AFC-1 frame. The arcball no longer re-aims the
    # camera at its anchor behind your back, so this really is the instrument
    # boresight now -- about 0.15 deg off the Didymos centre.
    sim.camera.pos = p_hera
    sim.camera.dir = m_afc_ej2k @ numpy.array([0.0, 0.0, 1.0])
    sim.camera.up = m_afc_ej2k @ numpy.array([1.0, 0.0, 0.0])


app = kalast.app.App()
app.config.width = 1020
app.config.height = 1020
app.config.color_mode = 0

# No shadow tuning here any more. shadow_normal_offset_scale, shadow_bias_scale
# and shadow_bias_minimum default to None, meaning "derive from the fitted
# light frustum and shadow_resolution" every frame. Set one only if you are
# chasing a suspected shadow artefact -- doing so pins that one value and
# leaves the others automatic.

spice.kclear()
spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/mk/hera_plan_local.tm")
et0 = spice.str2et("2026-11-05 00:00:00 UTC")
et = et0
simu_dt = 15.0 * 60.0
# Hera's own kernels run out before 2027-05-01: HERA_AFC-1 orientation (CK)
# stops at 2027-04-30T10:33:51 and the HERA SPK at 10:40, so sweeping to May
# throws SPKINSUFFDATA near the end of the run.
etf = spice.str2et("2027-04-30 00:00:00 UTC")
(p_sun, _lt) = spice.spkpos("SUN", et, "ECLIPJ2000", "none", "DIDYMOS")
(p_dimo, _lt) = spice.spkpos("DIMORPHOS", et, "ECLIPJ2000", "none", "DIDYMOS")
m_didy_ej2k = spice.pxform("DIDYMOS_FIXED", "ECLIPJ2000", et)
m_dimo_ej2k = spice.pxform("DIMORPHOS_FIXED", "ECLIPJ2000", et)

# The sun's projection side/near/far are gone too: they are fitted to the
# bodies' bounding box every frame. fovy stays, because it is a property of
# the instrument rather than something derivable from the scene.
app.simulation.camera.projection.fovy = 5.5 * RPD

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

# Optional, all defaulted -- shown here because a long export run is exactly
# where they matter. See notes/CONFIG_options.md.
#
# app.config.export_max_queued = 64   # cap frames queued for PNG encoding;
#                                     # 0 is unbounded and will eat all RAM
# app.config.export_sync = True       # block until each frame is on disk
# app.config.vsync = False            # uncap the loop when timing something
# app.config.shadow_pcf = 8           # soften shadow edges, (2N+1)^2 taps
# app.config.wireframe_mode = 2       # 1 wireframe only, 2 over the shading
# app.config.wireframe_width = 1.5

app.tick = tick
app.start()
spice.kclear()
