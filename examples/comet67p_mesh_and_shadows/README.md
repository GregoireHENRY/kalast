# Comet 67P from Rosetta's OSIRIS NAC: its shape and its shadows

Comet 67P/Churyumov-Gerasimenko through the narrow-angle camera of
Rosetta's OSIRIS, two ways:

- **Its shape, from its images** ([mesh.py](mesh.py)): made by
  `kalast.shape` from 354 of the images and ESA's SPICE kernels alone, as a
  spacecraft arriving at a body knows it -- no shape model to start from.
  The images' silhouettes carved into a hull, then maplets over it, coarse
  to fine, the mesh made again from them each round: 40 m, 20 m, then 10 m.
  Preusker et al.'s SHAP7 only measures the result.
- **Its images, drawn** ([shadows.py](shadows.py), [compare.py](compare.py)):
  57 of the images, July 2014 to August 2016, at phase angles of 2 to 120
  deg and from 9 to 2,900 km, drawn by kalast on SHAP7 and set beside the
  real ones -- shadows, pointing, brightness.

The files:

- [mesh.py](mesh.py), into `out/67p_from_images/`:
    - `fetch`: the frames of `frames.csv`, whole;
    - `hull`: the frames' silhouettes carved (`kalast.shape.Hull`) -- the
      sky in a frame is what is neither lit nor behind lit ground from the
      Sun -- each frame that shows a limb registered on the hull three
      times, then meshed in 20 m cubes, smoothed by 30 m: `hull.ply`, one
      closed piece, and each frame's pointing, `hull.npz`;
    - `tiles`: rounds of maplets of 31 x 31 points, laid once a stage --
      ten at 40 m, four at 20 m, three at 10 m. Each frame registered on
      each maplet; every frame's pointing and every maplet's place and
      height fitted to all the registrations at once
      (`kalast.shape.pointing`), the frames registered by their limbs
      holding theirs; each maplet solved again with the pointing held; all
      merged into the mesh made again (`kalast.shape.merge`):
      `40m_1.ply` to `10m_3.ply`, each a closed mesh, with its frames'
      pointing;
    - `compare`: the last mesh against SHAP7 -- as the kernels place both,
      then SHAP7 laid on it at its best.
- [frames.csv](frames.csv): `mesh.py`'s 354 frames, their data sets,
  distance (km) and phase (deg); `hull` 1 for the 336 the hull is carved
  from -- 60 to 600 km, phase 31 to 70 deg, from 178 of 180 directions
  about the nucleus -- 0 for 18 more that see what those see least.
- [shadows.py](shadows.py): the images of [images.csv](images.csv), saved
  to `out/67p_osiris/`; its docstring lists the options (shadow method,
  scattering law, shape model, the nucleus turned, ...).
- [compare.py](compare.py): those images beside kalast's -- shadows,
  pointing, brightness -- figures, one sheet of them all, and a table in
  `out/67p_osiris/figures/`.
- [images.csv](images.csv): `shadows.py`'s 57 images, their data sets, and
  where each one's terrain puts the nucleus against SPICE's pointing
  (`dx`, `dy`, in pixels), measured by `compare.py`.
- [67p_albedo.npz](67p_albedo.npz): the nucleus's albedo fitted to those
  images, in cubes of the surface 10 m on a side, for `shadows.py`'s
  `albedo`.
- [osiris.py](osiris.py): reads the images, and downloads them.

## Input data

The paths in the scripts are where these are kept here: set them to yours.

1. **The images**: downloaded on first use -- by `shadows.py`, or by
   `mesh.py fetch` -- from ESA's
   [Planetary Science Archive](https://archives.esac.esa.int/psa/ftp/INTERNATIONAL-ROSETTA-MISSION/OSINAC/),
   or NASA's [PDS Small Bodies Node](https://pdssbn.astro.umd.edu/holdings/)
   when ESA's is down, into `osiris.DATA` (set in [osiris.py](osiris.py)):
   38 MB a full frame, 1.9 GB for the 57, 13 GB for the 354. OSIRIS NAC
   level-4 products -- I/F, geometric distortion corrected -- of the
   `RO-C-OSINAC-4-*-67P-*-STR-REFL-V1.0` data sets.
2. **SPICE kernels**: the 18 that [rosetta.tm](rosetta.tm) lists, 550 MB,
   from ESA's [Rosetta set](https://spiftp.esac.esa.int/data/SPICE/ROSETTA/kernels/),
   in their folders there. Set its `PATH_VALUES`.
3. **The shape model**: Preusker et al. (2017)'s SHAP7, from ESA's Rosetta
   simulator server: in full,
   [cg-dlr_spg-shap7-v1.0.ply](http://comsim.esac.esa.int/rossim/SHAPE_MODEL_DRAFTS/SHAP7_8/SPG/cg-dlr_spg-shap7-v1.0_fullres-and-reductions_PLY/cg-dlr_spg-shap7-v1.0_fullres_PLY/cg-dlr_spg-shap7-v1.0.ply)
   (44M facets, 835 MB), and reduced,
   [cg-dlr_spg-shap7-v1.0_4Mfacets.ply](http://comsim.esac.esa.int/rossim/SHAPE_MODEL_DRAFTS/SHAP7_8/SPG/cg-dlr_spg-shap7-v1.0_fullres-and-reductions_PLY/cg-dlr_spg-shap7-v1.0_reductions_PLY/cg-dlr_spg-shap7-v1.0_4Mfacets.ply)
   (4M, 76 MB). `mesh.py` reads the first as it is, for `compare` only
   (`SHAP7` there). `shadows.py` loads OBJ: each made one once, beside it,
   by
   ```python
   import re, sys
   import numpy

   path = sys.argv[1]  # a binary PLY of triangles
   raw = open(path, "rb").read()
   end = raw.index(b"end_header\n") + len(b"end_header\n")
   nv, nf = (int(re.search(rb"element " + e + rb" (\d+)", raw[:end])[1]) for e in (b"vertex", b"face"))
   v = numpy.frombuffer(raw, "<f4", 3 * nv, end).reshape(-1, 3)
   f = numpy.frombuffer(raw, [("n", "u1"), ("i", "<i4", 3)], nf, end + 12 * nv)["i"]
   with open(path[:-4] + ".obj", "w") as fh:
       numpy.savetxt(fh, v, fmt="v %.6f %.6f %.6f")
       numpy.savetxt(fh, f + 1, fmt="f %d %d %d")
   ```

## Running

The shape:

```sh
python mesh.py fetch      # 13 GB, ten minutes at 20 MB/s
python mesh.py hull       # 15 minutes on an M1 Pro
python mesh.py tiles      # 40 m: 10 rounds of 12 minutes; 20 m: 4 of 17; 10 m: 3 of 40
python mesh.py compare
```

Each step keeps what it made; `tiles` starts again at the first round not
made. See `notes/2026-10-20_67p_from_images/` for how each part was found
and how near SHAP7 each round comes.

The images:

```sh
python shadows.py reference            # the 57, traced, with SHAP7 in full
python shadows.py reference hapke-fit  # the law refitted to these images
python shadows.py reference hapke-fit bounces=1 response albedo
                                       # and the comet's own light in its shadows,
                                       # the NAC's response, the albedo map
python shadows.py rays mask shap7-4m   # lit or not, the reduced model
python shadows.py reference N2016      # 2016's only
python compare.py                      # figures and table
```

On an M1 Pro with 32 GB, SHAP7 in full loads in 20 s and takes nearly all
the memory, 31 GB at its peak; an image is then about 20 s with `reference`,
the 57 in 12 minutes -- 32 with a bounce, 40 with the response and the
albedo map too. The 4M reduction needs 3.3 GB, and coarsens what the closer
images show.
