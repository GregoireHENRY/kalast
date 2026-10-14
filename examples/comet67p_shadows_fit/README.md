# Comet 67P from Rosetta's OSIRIS NAC

57 images of comet 67P/Churyumov-Gerasimenko from the narrow-angle camera of
Rosetta's OSIRIS, July 2014 to August 2016, at phase angles of 2 to 120 deg
and from 9 to 2,900 km, drawn by kalast and set beside the real ones.

- [main.py](main.py): the images of [images.csv](images.csv), saved to
  `out/67p_osiris/`; its docstring lists the options (shadow method,
  scattering law, shape model, the nucleus turned, ...).
- [compare.py](compare.py): the images beside kalast's -- shadows, pointing,
  brightness -- figures, one sheet of them all, and a table in
  `out/67p_osiris/figures/`.
- [osiris.py](osiris.py): reads the images, and downloads them.
- [images.csv](images.csv): the images, their data sets, and where each
  one's terrain puts the nucleus against SPICE's pointing (`dx`, `dy`, in
  pixels), measured by `compare.py`.
- [67p_albedo.npz](67p_albedo.npz): the nucleus's albedo fitted to these
  images, in cubes of the surface 10 m on a side, for `main.py`'s `albedo`.

## Input data

The paths in the scripts are where these are kept here: set them to yours.

1. **The images**: downloaded by `main.py` on first use, from NASA's
   [PDS Small Bodies Node](https://pdssbn.astro.umd.edu/), into
   `osiris.DATA`: 38 MB a full frame, 1.9 GB for the 57. OSIRIS NAC level-4
   products -- I/F, geometric distortion corrected -- of the
   `RO-C-OSINAC-4-*-67P-*-STR-REFL-V1.0` data sets.
2. **SPICE kernels**: the 18 that [rosetta.tm](rosetta.tm) lists, 550 MB,
   from ESA's [Rosetta set](https://spiftp.esac.esa.int/data/SPICE/ROSETTA/kernels/),
   in their folders there. Set its `PATH_VALUES`.
3. **The shape model**: Preusker et al. (2017)'s SHAP7, from ESA's Rosetta
   simulator server: in full,
   [cg-dlr_spg-shap7-v1.0.ply](http://comsim.esac.esa.int/rossim/SHAPE_MODEL_DRAFTS/SHAP7_8/SPG/cg-dlr_spg-shap7-v1.0_fullres-and-reductions_PLY/cg-dlr_spg-shap7-v1.0_fullres_PLY/cg-dlr_spg-shap7-v1.0.ply)
   (44M facets, 835 MB), and reduced,
   [cg-dlr_spg-shap7-v1.0_4Mfacets.ply](http://comsim.esac.esa.int/rossim/SHAPE_MODEL_DRAFTS/SHAP7_8/SPG/cg-dlr_spg-shap7-v1.0_fullres-and-reductions_PLY/cg-dlr_spg-shap7-v1.0_reductions_PLY/cg-dlr_spg-shap7-v1.0_4Mfacets.ply)
   (4M, 76 MB). kalast reads OBJ: each made one once, beside it, by
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

```sh
python main.py reference            # the 57, traced, with SHAP7 in full
python main.py reference hapke-fit  # the law refitted to these images
python main.py reference hapke-fit bounces=1 response albedo
                                    # and the comet's own light in its shadows,
                                    # the NAC's response, the albedo map
python main.py rays mask shap7-4m   # lit or not, the reduced model
python main.py reference N2016      # 2016's only
python compare.py                   # figures and table
```

On an M1 Pro with 32 GB, SHAP7 in full loads in 20 s and takes nearly all
the memory, 31 GB at its peak; an image is then about 20 s with `reference`,
the 57 in 12 minutes -- 32 with a bounce, 40 with the response and the
albedo map too. The 4M reduction needs 3.3 GB, and coarsens what the closer
images show.
