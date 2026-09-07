#!/usr/bin/env python

# The loop stays here rather than inside `app.start()`.
#
# `app.step()` draws one frame and returns False once the window closes, so
# what would have gone in `before_render` is simply written above it, and what
# would have gone in `after_render` below. A GPU result is requested on one
# side of a step and read on the other -- which is the only ordering rule the
# two callbacks ever existed to enforce.
#
# Needs no data beyond `res/`, so it runs on a fresh clone.

import numpy

from kalast.app import App, Hud

app = App()
app.config.width = 1024
app.config.height = 768
app.config.vsync = False
app.config.access_shadow_map = True
app.simulation.huds = [Hud("", size=16)]
app.simulation.sun.pos = [0.0, 20.0, 5.0]
app.simulation.camera.pos = [1.5778934, 1.9384689, 1.5082116]
# Full precision, not rounded: `camera.dir` and `camera.up` must be unit
# vectors, and a short vector aborts the process rather than raising -- the
# check fires inside winit's launch callback, which cannot unwind.
app.simulation.camera.dir = [-0.54051036, -0.6640262, -0.5166407]
app.simulation.camera.up = [-0.3261482, -0.40068075, 0.85620236]
app.simulation.load_mesh(
    path="res/plane_crater_1024-5000_h=0.437.obj", mat=numpy.eye(4), flatten=True
)

while app.step():
    sim = app.simulation
    it = sim.state.iteration

    # Before the next frame: move the Sun in a slow circle.
    a = it * 0.01
    sim.sun.pos = [20.0 * numpy.cos(a), 20.0 * numpy.sin(a), 8.0]

    # After the frame just drawn: its shadow map is readable now.
    shadow = sim.facet_shadow(0)
    lit = float((shadow < 0.5).mean()) if len(shadow) else 0.0
    sim.huds[0].text = f"it={it}  lit {lit * 100:.1f} %"

    if it >= 2000:
        app.close()
