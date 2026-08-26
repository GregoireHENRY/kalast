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



def after_render(sim: kalast.app.simulation.Simulation, dt: float):
    et = et0 + sim.state.iteration * simu_dt
    if et > etf:
        return

    # Available for every body each frame because config.facet_shadow is on.
    # Facets the shadow map reports as at least partly occluded from the Sun:
    # roughly half is simply the night side, and a rise above that baseline is
    # a mutual event. `1.0 - frac` is the lit fraction the thermophysical
    # boundary condition wants.
    didy = sim.facet_shadow(0)
    dimo = sim.facet_shadow(1)
    if didy is None or dimo is None:
        return

    if sim.state.iteration % 200 == 0:
        print(
            f"it={sim.state.iteration:6d} {spice.et2utc(et, 'C', 0)}  "
            f"shadowed: didymos={int((didy > 0.0).sum()):,} "
            f"dimorphos={int((dimo > 0.0).sum()):,}",
            flush=True,
        )


app = kalast.app.App()
app.config.width = 1020
app.config.height = 1020
app.config.color_mode = 0
# Per-facet solar occlusion for every body, every frame, readable from
# after_render. Off by default: it costs ~7 ms per body at this resolution,
# which only pays for itself if you actually consume the result.
app.config.facet_shadow = True
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

# A 100k `shadow_path` proxy is ~1.5x faster (notes/shadow_mesh_comparison/)
# and safe to combine with the facet query: the query still tests the real
# 3.1M facets, only the *depths* in the map come from the coarser occluder.
# Measured at the transit epoch, proxy vs full-res occluder: 99.98% of facets
# agree, 637 of 3,145,728 differ -- 60x smaller than the shadow map's own
# 1.28% disagreement with ray tracing, so it is well inside the noise.
mat = numpy.eye(4)
mat[:3, :3] = m_didy_ej2k
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
app.after_render = after_render
app.start()
spice.kclear()
