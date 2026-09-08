#!/usr/bin/env python

import numpy

import kalast


app = kalast.app.App()
app.simulation.config.debug_light_cube_show = True
# The Sun orbits at ds = 5 while the cube is 1 across, so the camera's
# automatic far plane -- fitted to the bodies -- sits well inside the Sun and
# clips the marker away for half the orbit. This includes it in that fit.
app.simulation.config.debug_light_cube_fit = True
app.simulation.config.wireframe_mode = 2
app.simulation.config.wireframe_color = [0.05, 0.05, 0.05, 1.0]

ds = 5.0
app.simulation.sun.pos = numpy.array([1.0, 0.0, 0.0]) * ds
app.simulation.camera.pos = [18.0, 5.0, 10.0]
app.simulation.camera.look_anchor()

app.simulation.load_mesh(path="res/cube.obj", mat=numpy.eye(4), flatten=True)
mesh = app.simulation.bodies[0].mesh
nf = len(mesh.facets)
print(f"Mesh cube loaded with {nf} facets")

while app.running:
    it = app.simulation.state.iteration

    a = it * 0.01
    app.simulation.sun.pos = [ds * numpy.cos(a), ds * numpy.sin(a), 0.0]

    app.step()
