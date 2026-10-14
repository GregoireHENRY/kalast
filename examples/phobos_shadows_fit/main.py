#!/usr/bin/env python
"""The Mars Express HRSC SRC images of Phobos in this folder, drawn by kalast.

Each `.img` at its IMAGE_TIME: Phobos where SPICE puts it, seen through the
SRC as the spacecraft pointed it -- turned by where the image's craters put
Phobos -- lit by the Sun, and with the reference, by Mars too. `compare.py`
sets them beside the real ones.

    python main.py [METHOD] [LAW] [MESH]

- METHOD: the shadows, `maps` (the default), `rays` or `reference`.
- LAW: `hapke` (Fornasier et al. 2024), `lambert`, or `mask` -- lit or not,
  for comparing shadows alone.
- MESH: `ernst` (Ernst et al. 2023, the default), `gaskell` (rot_shdw.TM's
  DSK) or `dlr`.
- `pck00011`: Phobos turned as NAIF's PCK00011 has it, not PCK00010 as
  rot_shdw.TM loads -- 0.6 to 0.8 deg apart at these dates. Always with
  `ernst`, whose frame it is.
- `spice`: the camera pointed as SPICE has it alone, without `POINTING`.
- `albedo`: Phobos's albedo as these four images have it, as was done for
  Deimos from AFC -- a map of the very images it is compared with: held
  out, each image's from the other three, it takes away half of what is
  left of the brightness.
- `libration=DEG`: Phobos's forced libration in longitude, PCK00011's last
  prime-meridian term (-1.143 deg), set to DEG: `libration=0` without it.

Frames go to out/phobos_hrsc/METHOD_LAW_MESH/IMAGE/.
"""

import glob
import os
import sys

import numpy
import spiceypy as spice

import hrsc
from kalast.app import App
from kalast.scattering import Hapke
from kalast.util import RPD

HERE = os.path.dirname(os.path.abspath(__file__))
DSK = "/Users/gregoireh/data/spice/mex/kernels/dsk"
MESHES = {
    # Ernst et al. (2023), from 2,382 images of six spacecraft, the SRC's
    # among them: 18 m facets, 12.6M of them, in PCK00011's frame.
    # https://sbmt.jhuapl.edu/shared-files/, v004.
    "ernst": "/Users/gregoireh/data/mesh/phobos/ernst2023/phobos_g_018m_spc_obj_0000n00000_v004.obj",
    # rot_shdw.TM's PHOBOS_M157_GAS_V01.BDS as an OBJ: Gaskell (2011), from
    # Viking images, 3.1M facets.
    "gaskell": DSK + "/PHOBOS_M157_GAS_V01.OBJ",
    # DLR's PHOBOS_K275_DLR_V02, from HRSC images (Willner et al. 2014),
    # 275k facets.
    "dlr": DSK + "/PHOBOS_K275_DLR_V02.OBJ",
}
# Where each image's craters put Phobos against SPICE's pointing, in pixels
# along the samples and the lines, which the camera is turned by: 15-21
# pixels along the samples in 2010-11, where Pasewaldt et al. (2015)'s
# positions of Phobos, checked against the stars, put it on the SRC's
# pointing rather than Phobos's ephemeris. `compare.py` measures them.
POINTING = {"h8870": (20.7, 14.1), "h8951": (15.4, -4.8), "h8963": (19.8, -2.2), "hc069": (-44.1, 44.5)}
# By name, in any order: the UI app passes arguments of its own.
METHOD = next((a for a in sys.argv if a in ("maps", "rays", "reference")), "maps")
LAW = next((a for a in sys.argv if a in ("hapke", "lambert", "mask")), "hapke")
MESH = next((a for a in sys.argv if a in MESHES), "ernst")
PCK00011 = "pck00011" in sys.argv or MESH == "ernst"
LIBRATION = next((float(a[10:]) for a in sys.argv if a.startswith("libration=")), None)
OUT = os.path.join("out", "phobos_hrsc", f"{METHOD}_{LAW}_{MESH}" + ("-pck00011" if PCK00011 else "")
                   + ("-spice" if "spice" in sys.argv else "") + ("-albedo" if "albedo" in sys.argv else "")
                   + (f"-libration{LIBRATION:g}" if LIBRATION is not None else ""))
IFOV = 9e-3 / 984.76  # the SRC's pixel over its focal length, MEX_HRSC_V09.TI

spice.kclear()
spice.furnsh(os.path.join(HERE, "phobos_hrsc.tm"))
if PCK00011:
    # NAIF's generic kernels, https://naif.jpl.nasa.gov/pub/naif/generic_kernels/pck/
    spice.furnsh("/Users/gregoireh/data/spice/hera/kernels/pck/pck00011.tpc")
    if LIBRATION is not None:
        pm = list(spice.gdpool("BODY401_NUT_PREC_PM", 0, 5))
        pm[4] = LIBRATION
        spice.pdpool("BODY401_NUT_PREC_PM", pm)

app = App()
c = app.simulation.config
c.axes.style = "off"
c.wireframe.mode = 0
c.shading.srgb_mode = "linear"  # a pixel is exposure x I/F x 255
# Bright and unclipped: Lambert of colour 1 reaches I/F 1 face on, Hapke
# about 0.05 here; `mask` lights every lit pixel to white.
c.light.exposure = {"hapke": 12.0, "lambert": 0.8, "mask": 1e4}[LAW]
c.shadows.method = METHOD
# A pixel on a shadow's edge is lit by half its samples or so: 1024 put it
# within about 1.5 %, where the reference's own stop wants tens of thousands.
c.reference.max_samples = 1024
c.export.sync, c.export.hud = True, False
sim = app.simulation
sim.load_mesh(path=MESHES[MESH])
if "albedo" in sys.argv:
    # Relative albedo fitted to the four images through the SRC's response
    # (notes/2026-10-12_phobos_hrsc_shadows): simple cylindrical, 0.25 deg a
    # pixel, the first row at 90 N, columns east from -180, NaN where they
    # did not see Phobos, which keeps 1.
    albedo = numpy.load(os.path.join(HERE, "phobos_src_albedo.npz"))["albedo"]
    sim.bodies[0].mesh.colors_from_map(albedo.astype(numpy.float32))
    sim.bodies[0].mesh.update_gpu_colors()
if LAW == "hapke":
    # Hapke 2012 fits of HRSC images, Fornasier et al. (2024), between their
    # green and red filters for the SRC's 600 nm.
    sim.bodies[0].scattering = Hapke(w=0.0736, b=0.265, c=1.0, b0=2.283, h=0.0573, theta_bar=24.0 * RPD, k=1.19)
# The reference with the light of Mars on Phobos's night side: Mars in the
# scene, a Lambert sphere of its Bond albedo, and light bounced once; the
# Sun a disc.
MARS = METHOD == "reference" and LAW != "mask"
if MARS:
    c.reference.bounces = 1
    c.light.sun_as_point = False
    sim.load_mesh(path=os.path.join(HERE, "..", "..", "res", "ico5.obj"))
    sim.bodies[1].mesh.colors[:] = 0.25
    sim.bodies[1].mesh.update_gpu_colors()
    mars_radii = spice.bodvrd("MARS", "RADII", 3)[1]

for path in sorted(glob.glob(os.path.join(HERE, "*.img"))):
    name = os.path.basename(path)[:10]
    _, label = hrsc.read(path)
    et = spice.str2et(label["time"])

    # The scene in the camera's frame, MEX_HRSC_SRC: positions in J2000
    # first, since one relative to a frame centred on the camera would want
    # the camera's own ephemeris.
    to_src = numpy.array(spice.pxform("J2000", "MEX_HRSC_SRC", et))
    p_phobos, lt = spice.spkpos("PHOBOS", et, "J2000", "LT+S", "MEX")
    p_sun, _ = spice.spkpos("SUN", et, "J2000", "LT+S", "MEX")
    sim.bodies[0].mat[:3, :3] = to_src @ numpy.array(spice.pxform("IAU_PHOBOS", "J2000", et - lt))
    sim.bodies[0].mat[:3, 3] = to_src @ p_phobos
    sim.sun.pos = (to_src @ p_sun).tolist()
    if MARS:
        p_mars, lt_mars = spice.spkpos("MARS", et, "J2000", "LT+S", "MEX")
        sim.bodies[1].mat[:3, :3] = to_src @ numpy.array(spice.pxform("IAU_MARS", "J2000", et - lt_mars)) @ numpy.diag(mars_radii)
        sim.bodies[1].mat[:3, 3] = to_src @ p_mars

    # The image is a window of the 1024 x 1024 CCD, its centre off the
    # boresight, and stored turned half a turn from the IK's diagram: a
    # sample along +X, a line along +Y.
    sample = label["first_sample"] + (label["samples"] - 1) / 2.0
    line = label["first_line"] + (label["lines"] - 1) / 2.0
    dx, dy = (0.0, 0.0) if "spice" in sys.argv else POINTING[name[:5]]
    centre = numpy.array([(sample - 512.5 - dx) * IFOV, (line - 512.5 - dy) * IFOV, 1.0])
    c.image.width, c.image.height = label["samples"], label["lines"]
    sim.camera.pos = [0.0, 0.0, 0.0]
    sim.camera.dir = (centre / numpy.linalg.norm(centre)).tolist()
    sim.camera.up = [0.0, -1.0, 0.0]
    sim.camera.projection.fovy = label["lines"] * IFOV

    c.export.dir = os.path.join(OUT, name)
    if METHOD == "reference":
        app.step()
        while not sim.reference_done and app.running:
            app.step()
        if sim.reference_done:
            os.makedirs(c.export.dir, exist_ok=True)
            numpy.save(os.path.join(c.export.dir, "mean.npy"), numpy.asarray(sim.reference_image(), numpy.float32))
    else:
        for _ in range(30):
            app.step()
    sim.export_once()
    for _ in range(3):
        app.step()
    print(f"{name}: {numpy.linalg.norm(p_phobos):.0f} km from Phobos")

spice.kclear()
