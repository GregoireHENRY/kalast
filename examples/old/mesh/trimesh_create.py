# import numpy
import trimesh


trimesh.util.attach_to_log()
mesh = trimesh.creation.icosphere(subdivisions=3)
# mesh = trimesh.creation.uv_sphere()

print(mesh.faces.shape)
print(mesh.vertices.shape)

mesh.export("ico.obj")

# mesh.show()
