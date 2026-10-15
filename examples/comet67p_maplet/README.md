# Comet 67P's relief from its images

A patch of comet 67P/Churyumov-Gerasimenko 120 m square, in Wosret beside
Abydos where Philae came to rest, solved at 0.5 m from 209 images of
Rosetta's OSIRIS narrow-angle camera by `kalast.shape`: the shading, the
cast shadows and the parallax under every Sun the images were taken in,
held to Preusker et al.'s SHAP7 shape model above about 12 m. Thirteen
images from five days are held out, and the maplet is checked on them.

- [main.py](main.py): downloads the images on first use, solves the maplet,
  scores it and SHAP7 on the held-out images, and writes to
  `out/67p_maplet/`:
    - `maplet.npz`: its heights and albedo;
    - `maplet.obj`: the maplet as a mesh, in km in the comet's frame
      (`67P/C-G_CK`), to load as SHAP7 is;
    - `held_out.png`: three held-out images beside SHAP7 and the maplet
      drawn in them.
- [images.csv](images.csv): the images, their data sets, the lines of each
  that see the patch (`first`, `last`), their scale at the patch
  (`m_per_px`), phase, and whether they are held out (`held`).

## Input data

The paths in the scripts are where these are kept here: set them to yours.

1. **The images**: downloaded by `main.py` on first use, from ESA's
   [Planetary Science Archive](https://archives.esac.esa.int/psa/ftp/INTERNATIONAL-ROSETTA-MISSION/OSINAC/),
   only the lines that see the patch: 951 MB for the 222, into
   `rows/` of `osiris.DATA` (set in
   [../comet67p_shadows_fit/osiris.py](../comet67p_shadows_fit/osiris.py)).
   OSIRIS NAC level-4 products -- I/F, geometric distortion corrected -- of
   2014 and 2016, at 0.05-4 m a pixel.
2. **SPICE kernels**: those of
   [../comet67p_shadows_fit/rosetta.tm](../comet67p_shadows_fit/rosetta.tm),
   from ESA's [Rosetta set](https://spiftp.esac.esa.int/data/SPICE/ROSETTA/kernels/),
   550 MB; its `PATH_VALUES` set to where they are.
3. **SHAP7 in full**, as a binary PLY:
   [cg-dlr_spg-shap7-v1.0.ply](http://comsim.esac.esa.int/rossim/SHAPE_MODEL_DRAFTS/SHAP7_8/SPG/cg-dlr_spg-shap7-v1.0_fullres-and-reductions_PLY/cg-dlr_spg-shap7-v1.0_fullres_PLY/cg-dlr_spg-shap7-v1.0.ply)
   (44M facets, 835 MB), `SHAP7` in `main.py`.

## Running

```sh
python main.py
```

On an M1 Pro, 5 minutes the first time, the downloads included; 76 s after.
It prints each held-out image's correlation with SHAP7 and with the maplet
drawn in it, at the image's pixels and at the maplet's own scale:

| | held-out images, median correlation | at the maplet's scale |
|---|---|---|
| SHAP7 | 0.26 | 0.28 |
| the maplet | 0.46 | 0.48 |

To see the maplet in kalast, as a body of its own in the comet's frame:

```python
app.simulation.load_mesh(path="out/67p_maplet/maplet.obj")
```

## What it does

- **The images** are those of the archive's 15,343 orange-filter NAC
  frames that see the patch lit, at most 4 m a pixel, from 2014 and 2016:
  2015's, around perihelion -- a surface since changed, or pointing from
  100-200 km -- made the maplet worse.
- **Each image's pointing**, from SPICE, is off by metres on the ground;
  `reach=0.012` lets each image's registration search 12 m, coarse to fine,
  as it joins the first level fine enough for it, against the maplet as the
  coarser levels left it.
- **Each image weighs by its Sun's incidence**, as SPC weighs its images: a
  high Sun's shading says more of the albedo than of the slopes.
- The notes that built and checked this --
  `notes/2026-10-14_shape_from_images/` and
  `notes/2026-10-15_shape_direct/` -- have the method, its checks on
  synthetic terrain, and the comparison with LAM's MPCD model of Abydos,
  which the maplet matches on the held-out images.
