import numpy
import trimesh

import kalast
from kalast.util import RPD


def find_closest(
    m: numpy.ndarray,
    refv1: float,
    i1: int,
    threshold: int,
    refv2: float,
    i2: int,
    N: int = 1,
) -> list[tuple[int, numpy.ndarray]]:
    ii = numpy.where(numpy.abs(refv1 - m[:, i1]) < threshold)[0]
    jj = numpy.argsort(numpy.abs(refv2 - m[ii, i2]))[:N]
    kk = ii[jj]
    return [(kkk, m[kkk]) for kkk in kk]


trimesh.util.attach_to_log()
mesh = trimesh.creation.icosphere(subdivisions=3)
# mesh = trimesh.creation.uv_sphere()

print(mesh.faces.shape)
print(mesh.vertices.shape)

radii = numpy.array([13.0, 11.4, 9.1])
mesh.vertices *= radii
sph = numpy.array([kalast.util.cart2sph(v) for v in mesh.triangles_center])

faces = find_closest(
    sph, refv1=0.0, i1=0, threshold=10.0 * RPD, refv2=1.0 * RPD, i2=1, N=15
)
for ii, face in faces:
    print(ii, face)

# 1264 is 4°lon, 0°lat

mesh.unmerge_vertices()
mesh.visual.face_colors[1264] = [255, 0, 0, 255]
mesh.visual.face_colors[0] = [0, 0, 255, 255]

# mesh.export("phobos.obj")

# mesh.show()
