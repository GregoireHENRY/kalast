from pathlib import Path

import numpy
import trimesh

import kalast
import plot

kalast.plot.style.load()

surf = kalast.astro.Surface()
surf.mesh = trimesh.load("phobos.obj")
nface = surf.nfaces()
nvert = surf.nfaces()
sph = numpy.array([kalast.util.cart2sph(v) for v in surf.mesh.triangles_center])
print(f"nfaces={nface} nvert={nvert}")

path_out = Path("out")
ts = numpy.load(path_out / "ts.npy")
z = numpy.load(path_out / "z.npy")
tmp1 = numpy.load(path_out / "t_last.npy")
tmp__ = numpy.load(path_out / "t_surf.npy")
tmp2 = numpy.load(path_out / "t_depth.npy")

plot.smap(surf, sph, tmp1[:, 0], 160, 300, path_out)

kalast.plot.style.load()
plot.daily_surf(ts, tmp2[:, 0], path_out, (0, 24))
plot.depth(z * 100, tmp2[:360:30, :], path_out, (z[-1] * 100, 0))
plot.smap(surf, sph, tmp1[:, 0], 160, 300, path_out)
