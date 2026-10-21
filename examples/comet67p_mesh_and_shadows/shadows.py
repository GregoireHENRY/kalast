#!/usr/bin/env python
"""Rosetta's OSIRIS NAC images of comet 67P/Churyumov-Gerasimenko listed in
`images.csv`, drawn by kalast.

Each image at mid-exposure: the nucleus where SPICE puts it and turned as
its attitude kernel has it, seen through the NAC as the spacecraft pointed
it -- moved by where the image's terrain puts the nucleus -- and lit by the
Sun. `compare.py` sets them beside the real ones.

    python shadows.py [METHOD] [LAW] [MESH] [NAME ...]

- METHOD: the shadows, `maps` (the default), `rays` or `reference`.
- LAW: `hapke` (Fornasier et al. 2015), `hapke-fit` (refitted to these
  images, below), `lambert`, or `mask` -- lit or not, for comparing shadows
  alone.
- MESH: `shap7` (the default) or `shap7-4m`, below.
- `spice`: the camera pointed as SPICE has it alone, without `images.csv`'s
  `dx` and `dy`.
- `bounces=N`: with the reference, sunlight bounced N times off the
  surface, what lights ground in shadow -- the night side seen up close.
- `response`: the image as the NAC records it, through its response fitted
  to these images (`NAC_RESPONSE`).
- `albedo`: the nucleus's albedo as these images have it, in cubes of the
  surface 10 m on a side (`67p_albedo.npz`), 1 where none saw it.
- `spin=DEG`: the nucleus turned DEG further about its spin axis, for what
  the shadows say of its rotation.
- `light=DEG`: the light alone as the nucleus turned DEG further would have
  it, the Sun turned the other way about the spin axis, the surface where it
  is: the shadows' own part, which pointing cannot take up.
- NAME: only the images whose names start so, as `N2016`.

Each image is downloaded on first use into `osiris.DATA`, 38 MB a frame.
Frames go to out/67p_osiris/METHOD_LAW_MESH/IMAGE/, with the facets' ids
(ids.npy) for `compare.py` and, from the reference, its floats (mean.npy).
"""

import csv
import os
import sys

import numpy
import spiceypy as spice

import osiris
from kalast.app import App
from kalast.scattering import Hapke
from kalast.util import RPD

HERE = os.path.dirname(os.path.abspath(__file__))
SHAP7 = "/Users/gregoireh/data/mesh/67p/shap7"
MESHES = {
    # Preusker et al. (2017)'s SHAP7, from 1531 OSIRIS NAC images by
    # stereophotogrammetry: the whole nucleus in 44M facets, 1-1.5 m apart.
    # About 30 GB of memory to draw.
    "shap7": SHAP7 + "/cg-dlr_spg-shap7-v1.0.obj",
    # Its reduction to 4M facets, for smaller machines: coarser than most
    # of these images' pixels.
    "shap7-4m": SHAP7 + "/cg-dlr_spg-shap7-v1.0_4Mfacets.obj",
}
# By name, in any order: the UI app passes arguments of its own.
METHOD = next((a for a in sys.argv if a in ("maps", "rays", "reference")), "maps")
LAW = next((a for a in sys.argv if a in ("hapke", "hapke-fit", "lambert", "mask")), "hapke")
MESH = next((a for a in sys.argv if a in MESHES), "shap7")
SPIN = next((float(a[5:]) for a in sys.argv if a.startswith("spin=")), 0.0)
LIGHT = next((float(a[6:]) for a in sys.argv if a.startswith("light=")), 0.0)
BOUNCES = next((int(a[8:]) for a in sys.argv if a.startswith("bounces=")), 0)
NAMES = [a for a in sys.argv[1:] if a.startswith("N20")]
OUT = os.path.join("out", "67p_osiris", f"{METHOD}_{LAW}_{MESH}" + ("-spice" if "spice" in sys.argv else "")
                   + (f"-spin{SPIN:g}" if SPIN else "") + (f"-light{LIGHT:g}" if LIGHT else "")
                   + (f"-bounces{BOUNCES}" if BOUNCES else "") + ("-response" if "response" in sys.argv else "")
                   + ("-albedo" if "albedo" in sys.argv else ""))
IFOV = 18.82e-6  # the NAC's pixel, geometric distortion corrected, ROS_OSIRIS_V17.TI
# The NAC's response fitted to the six of these images from 330 km and more
# away from perihelion, whose coma it would take for a halo, SHAP7's metre
# well under their pixels (notes/2026-10-14_67p_response_albedo): a Gaussian
# core, sigma in pixels, and a share of the light in an exponential halo, its
# scale in pixels. The archive's products have the Sun's stray light taken
# out, not the camera's own wings and ghosts.
NAC_RESPONSE = dict(core=0.97, halo_share=0.133, halo_scale=73.6)

spice.kclear()
spice.furnsh(os.path.join(HERE, "rosetta.tm"))

app = App()
c = app.simulation.config
c.axes.style = "off"
c.wireframe.mode = 0
c.shading.srgb_mode = "linear"  # a pixel is exposure x I/F x 255
# Bright and unclipped: 67P's I/F stays under 0.08; `mask` lights every lit
# pixel to white.
c.light.exposure = {"hapke": 12.0, "hapke-fit": 12.0, "lambert": 0.8, "mask": 1e4}[LAW]
c.shadows.method = METHOD
# The Sun's disc spans 0.3 deg or less at 1.2-3.6 au, a shadow's penumbra a
# pixel or two here: 64 samples a pixel draw it within a few percent.
c.reference.max_samples = 64
c.reference.bounces = BOUNCES
c.export.sync, c.export.hud = True, False
if "response" in sys.argv:
    c.response.enabled = True
    for k, v in NAC_RESPONSE.items():
        setattr(c.response, k, v)
# The images are stored with their first line along the NAC's +X, kalast's
# up: mirrored top to bottom against kalast's view.
c.image.flip_y = True
sim = app.simulation
sim.load_mesh(path=MESHES[MESH])
if LAW == "hapke":
    # Fornasier et al. (2015)'s Hapke 2012 fit of the NAC's images in its
    # orange filter (649 nm), at phases 1.3 to 54 deg.
    sim.bodies[0].scattering = Hapke(w=0.034, b=0.42, c=1.0, b0=2.25, h=0.061, theta_bar=28.0 * RPD, k=1.2)
elif LAW == "hapke-fit":
    # The same refitted to 46 of these images, 2 to 94 deg, on SHAP7 in full
    # (notes/2026-10-13_67p_osiris_shadows): w, b, theta_bar and B0 refitted,
    # h and k kept. Fornasier et al.'s darkens too fast with phase beyond its
    # 54 deg: kalast's I/F 1.6 to 2 times too low at 80-94 deg. Through the
    # NAC's response, whose halo takes light off lit ground, 8 % more
    # (notes/2026-10-14_67p_response_albedo).
    w = 0.0385 if "response" in sys.argv else 0.0356
    sim.bodies[0].scattering = Hapke(w=w, b=0.276, c=1.0, b0=4.73, h=0.061, theta_bar=24.4 * RPD, k=1.2)
if "albedo" in sys.argv:
    # Each facet's colour the albedo of the cube its centre falls in: the
    # images' I/F over kalast's, fitted with a factor per image for what the
    # law leaves (notes/2026-10-14_67p_response_albedo).
    found = numpy.load(os.path.join(HERE, "67p_albedo.npz"))
    keys = numpy.cumsum(found["step"], dtype=numpy.int64)  # the cubes seen, as steps from one to the next
    values = found["albedo"].astype(numpy.float32)
    mesh = sim.bodies[0].mesh
    positions, corners = numpy.asarray(mesh.positions), numpy.asarray(mesh.indices).reshape(-1, 3)
    albedo = numpy.ones(len(corners), numpy.float32)
    for s in range(0, len(corners), 4_000_000):
        cube = numpy.floor((positions[corners[s : s + 4_000_000]].mean(axis=1) - found["origin"]) / found["cell"]).astype(numpy.int64)
        key = cube[:, 0] + found["dims"][0] * (cube[:, 1] + found["dims"][1] * cube[:, 2])
        at = numpy.minimum(numpy.searchsorted(keys, key), len(keys) - 1)
        albedo[s : s + 4_000_000] = numpy.where(keys[at] == key, values[at], 1.0)
    mesh.colors[:] = albedo[:, None]
    mesh.update_gpu_colors()

for image in csv.DictReader(open(os.path.join(HERE, "images.csv"))):
    name = image["product"][:16]
    if NAMES and not any(name.startswith(n) for n in NAMES):
        continue
    _, label = osiris.read(osiris.fetch(image["dataset"], image["product"]))
    et = spice.str2et(label["start"]) + label["exposure"] / 2

    # The scene in the camera's frame, ROS_OSIRIS_NAC: positions in J2000
    # first, since one relative to a frame centred on the camera would want
    # the camera's own ephemeris.
    to_nac = numpy.array(spice.pxform("J2000", "ROS_OSIRIS_NAC", et))
    p_comet, lt = spice.spkpos("CHURYUMOV-GERASIMENKO", et, "J2000", "LT+S", "ROSETTA")
    p_sun, _ = spice.spkpos("SUN", et, "J2000", "LT+S", "ROSETTA")
    body = numpy.array(spice.pxform("67P/C-G_CK", "J2000", et - lt))
    sim.bodies[0].mat[:3, :3] = to_nac @ body @ numpy.array(spice.axisar([0.0, 0.0, 1.0], SPIN * RPD))
    sim.bodies[0].mat[:3, 3] = to_nac @ p_comet
    turn = body @ numpy.array(spice.axisar([0.0, 0.0, 1.0], -LIGHT * RPD)) @ body.T
    sim.sun.pos = (to_nac @ (numpy.array(p_comet) + turn @ (numpy.array(p_sun) - p_comet))).tolist()

    # A window of the CCD has its centre off the boresight, its samples
    # counted against the stored image's; the lines along the NAC's +X, the
    # samples along +Y.
    (x0, x1), (y0, y1) = label["x"], label["y"]
    sample = 0.0 if label["samples"] == 2048 else 1024 - (x0 + x1) / 2
    line = 0.0 if label["lines"] == 2048 else (y0 + y1) / 2 - 1024
    dx, dy = (0.0, 0.0) if "spice" in sys.argv else (float(image["dx"]), float(image["dy"]))
    centre = numpy.array([(line - dy) * IFOV, (sample - dx) * IFOV, 1.0])
    c.image.width, c.image.height = label["samples"], label["lines"]
    sim.camera.pos = [0.0, 0.0, 0.0]
    sim.camera.dir = (centre / numpy.linalg.norm(centre)).tolist()
    sim.camera.up = [1.0, 0.0, 0.0]
    sim.camera.projection.fovy = label["lines"] * IFOV

    c.export.dir = os.path.join(OUT, name)
    os.makedirs(c.export.dir, exist_ok=True)
    if METHOD == "reference":
        app.step()
        while not sim.reference_done and app.running:
            app.step()
        if sim.reference_done:
            numpy.save(os.path.join(c.export.dir, "mean.npy"), numpy.asarray(sim.reference_image(), numpy.float32))
    else:
        for _ in range(30):
            app.step()
    sim.export_once()
    for _ in range(3):
        app.step()
    # Which facet each pixel sees: the nucleus's outline, lit or not.
    sim.request_facet_id()
    app.step()
    ids = sim.facet_id_map()
    if ids is not None:
        numpy.save(os.path.join(c.export.dir, "ids.npy"), numpy.asarray(ids[0], numpy.uint32))
    print(f"{name}: {label['distance']:.1f} km, phase {label['phase']:.1f} deg", flush=True)

spice.kclear()
