#!/usr/bin/env python

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


app = kalast.app.App()
app.config.width = 1020
app.config.height = 1020
app.config.color_mode = 0
app.config.shadow_normal_offset_scale = 2e-4
app.config.shadow_bias_scale = 1e-3
app.config.shadow_bias_minimum = 5e-4

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

app.simulation.sun.projection.side = 2.0
app.simulation.sun.projection.near = 0.1
app.simulation.sun.projection.far = 100.0
app.simulation.camera.projection.fovy = 5.5 * RPD

mat = numpy.eye(4)
mat[:3, :3] = m_didy_ej2k
# shadow_path renders a 100k-facet stand-in into the shadow map instead of
# the full 3.1M mesh: 1.5x faster overall, and 9 of 1,040,400 pixels differ
# from the full-resolution shadow. See notes/BENCH_shadow_mesh.md.
app.simulation.load_mesh(
    path="/Users/gregoireh/data/spice/hera/kernels/dsk/g_01165mm_spc_obj_didy_0000n00000_v003.obj",
    mat=mat,
    flatten=True,
    shadow_path="/Users/gregoireh/data/mesh/didymos/g_01165mm_spc_obj_didy_0000n00000_v003_decimated_100k.obj",
)

mat = numpy.eye(4)
mat[:3, 3] = p_dimo
mat[:3, :3] = m_dimo_ej2k
app.simulation.load_mesh(
    path="/Users/gregoireh/data/spice/hera/kernels/dsk/g_00243mm_spc_obj_dimo_0000n00000_v004.obj",
    mat=mat,
    flatten=True,
    shadow_path="/Users/gregoireh/data/mesh/dimorphos/g_00243mm_spc_obj_dimo_0000n00000_v004_decimated_100k.obj",
)
app.before_render = before_render
app.start()
spice.kclear()
