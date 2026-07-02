#!/usr/bin/env python

import numpy

import kalast


app = kalast.app.App()

# app.config.debug_app = True
# app.config.debug_window = True
# app.config.debug_simulation = True
app.config.debug_depth_show = True

app.config.width = 800
app.config.height = 600

app.config.background = [0.0, 0.0, 0.0, 0.0]

app.config.shader_color_mode = 1

# Set camera pos/up
# Different methods to set dir
app.simulation.camera.pos = [0.0, 1.0, 2.0]
app.simulation.camera.up = [0.0, 1.0, 0.0]

# method 1: compute dir yourself
dir_ = numpy.array([0.0, -1.0, -2.0])
app.simulation.camera.dir = dir_ / numpy.linalg.norm(dir_)

if False:
    # method 2: set anchor and look at it
    # anchor is origin by default, so not needed to write it, but to show you:
    app.simulation.camera.anchor = [0.0, 0.0, 0.0]
    app.simulation.camera.look_anchor()

if False:
    # method 3: same as method 2 but one less line code if anchor is not origin
    app.simulation.camera.set_target([0.0, 0.0, 0.0])

# can't see object for some reason i don't understand
# app.simulation.camera.projection.set_orthographic()

# Camera projection is perspective by default with ~20°
# app.simulation.camera.projection.set_perspective()
app.simulation.camera.projection.fovy = 45.0 * kalast.util.RPD

# Pentagon with 3 triangles
vertices = [
    kalast.mesh.Vertex(
        pos=[-0.0868241, 0.49240386, 0.0],
        tex=[0.4131759, 0.00759614],
        color=[0.5, 0.0, 0.5],
    ),
    kalast.mesh.Vertex(
        pos=[-0.49513406, 0.06958647, 0.0],
        tex=[0.0048659444, 0.43041354],
        color=[0.5, 0.0, 0.5],
    ),
    kalast.mesh.Vertex(
        pos=[-0.21918549, -0.44939706, 0.0],
        tex=[0.28081453, 0.949397],
        color=[0.5, 0.0, 0.5],
    ),
    kalast.mesh.Vertex(
        pos=[0.35966998, -0.3473291, 0.0],
        tex=[0.85967, 0.84732914],
        color=[0.5, 0.0, 0.5],
    ),
    kalast.mesh.Vertex(
        pos=[0.44147372, 0.2347359, 0.0],
        tex=[0.9414737, 0.2652641],
        color=[0.5, 0.0, 0.5],
    ),
]
indices = [0, 1, 4, 1, 2, 4, 2, 3, 4]
penta = kalast.mesh.Mesh(vertices=vertices, indices=indices)

mat = numpy.eye(4, dtype=numpy.float32)
# mat[0:3, -1] = [0.0, 0.0, 0.0]
# mat[0:3, 0:3] = 2d rot
app.simulation.add_body(penta, mat=mat)

mat[0:3, -1] = [0.5, -2.5, -5.0]
app.simulation.add_body(penta, mat=mat)

def tick(sim):
    # print(f"#{sim.state.iteration}")

    if False:
        # Update FOV at a specific iteration
        if sim.state.iteration == 100:
            sim.camera.projection.fovy = 20.0 * kalast.util.RPD
            print("#100")

    if False:
        # Update FOV at every iterations until loop at 180°
        sim.camera.projection.fovy = (sim.state.iteration * kalast.util.RPD) % (
            kalast.util.PI
        )


app.tick = tick
app.start()
