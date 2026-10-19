# Comet 67P whole, in tiles

Comet 67P/Churyumov-Gerasimenko finer than its shape model wherever its
images allow: Preusker et al.'s SHAP7 under 7,364 tiles, each at the finest
spacing that at least five of Rosetta's OSIRIS narrow-angle images resolve
-- 0.5, 1 or 2 m -- and solved by `kalast.shape` from up to 70 of the
images that see it, as `../comet67p_maplet/` solves one patch: 6,332 of
241 x 241 points, and 1,032 of 121 where the surface turns too much for a
grid that size -- the neck, the cliffs. Eight days of images are left out
of every tile, to score it on.

- [main.py](main.py): fetches each image's lines its tiles need, solves the
  tiles in two passes, several at once, and scores them on the held-out
  images, into `out/67p_tiles/`:
    - `first/t#####.npz`: a 241-point tile, each image registered on its
      own -- its heights, albedo and SHAP7's heights under it
      (`reference`), its centre, axes and spacing, and its images'
      registrations;
    - `pointing.npz`: each image's pointing, from its registrations in all
      the first pass's tiles;
    - `second/t#####.npz`: every tile again, each image's pointing held --
      the result;
    - `scores.csv`: each tile's correlation with up to three held-out
      images, SHAP7's beside it.
- [tiles.csv](tiles.csv): the tiles -- centre and normal in the comet's
  frame (km, `67P/C-G_CK`), spacing, points a side (`size`), SHAP7's lowest
  and highest heights under the tile (`low`, `high`) -- and the images each
  is solved from (`images`) and scored on (`held`), rows of
- [images.csv](images.csv): the 5,334 images, their data sets, and whether
  they are of a held-out day.

## Input data

The paths in the scripts are where these are kept here: set them to yours.

1. **The images**: downloaded by `main.py fetch`, from ESA's
   [Planetary Science Archive](https://archives.esac.esa.int/psa/ftp/INTERNATIONAL-ROSETTA-MISSION/OSINAC/),
   only the lines the tiles fall on, in half floats: about 30 GB for the
   whole comet, into `LINES`, and their labels into `labels/` of
   `osiris.DATA` (set in
   [../comet67p_shadows_fit/osiris.py](../comet67p_shadows_fit/osiris.py)).
2. **SPICE kernels**: those of
   [../comet67p_shadows_fit/rosetta.tm](../comet67p_shadows_fit/rosetta.tm),
   from ESA's [Rosetta set](https://spiftp.esac.esa.int/data/SPICE/ROSETTA/kernels/),
   550 MB; its `PATH_VALUES` set to where they are.
3. **SHAP7**, as binary PLY files, in full -- each tile's start -- and
   reduced to 4M facets -- what shadows a tile or hides it, beyond it:
   [cg-dlr_spg-shap7-v1.0.ply](http://comsim.esac.esa.int/rossim/SHAPE_MODEL_DRAFTS/SHAP7_8/SPG/cg-dlr_spg-shap7-v1.0_fullres-and-reductions_PLY/cg-dlr_spg-shap7-v1.0_fullres_PLY/cg-dlr_spg-shap7-v1.0.ply)
   (835 MB) and
   [cg-dlr_spg-shap7-v1.0_4Mfacets.ply](http://comsim.esac.esa.int/rossim/SHAPE_MODEL_DRAFTS/SHAP7_8/SPG/cg-dlr_spg-shap7-v1.0_fullres-and-reductions_PLY/cg-dlr_spg-shap7-v1.0_reductions_PLY/cg-dlr_spg-shap7-v1.0_4Mfacets.ply),
   `SHAP7` and `SHAP7_4M` in `main.py`.

## Running

```sh
python main.py fetch          # 30 GB, half an hour at 20 MB/s
python main.py first          # the 241-point tiles: a day on an M1 Pro, five at once
python main.py pointing       # each image's pointing, from all of them: a minute
python main.py second         # every tile again: a day
python main.py score
```

`fetch`, `first`, `second` and `score` take a range of tiles, `A B` for
tiles A to B-1 -- a part of the comet, or a share for another machine --
and start again where they stopped; `pointing` wants all the first pass.
`WORKERS` tiles are solved at once: each takes one to two cores, and four
fill an eight-core machine.

## What it does

- **The tiles**: SHAP7's surface in 20 m cubes, each image within 215 km
  against it -- what it sees lit (`Surface.sees`) -- and each cube's finest
  spacing: five images as fine as it, from two Sun directions, and ten
  within twice it from three. `Surface.tiles` lays them, finest first, a
  fifth of each overlapping its neighbours.
- **Each tile's images**: those that see a third of it lit, finest first,
  each kept unless one already kept is within 10 deg of its Sun and its
  view, 1.5x of its scale, and sees the same ground (`kalast.shape.choose`,
  as LAM's MPCD keeps images). 2014 and 2016; 2015's -- around perihelion,
  the surface since changed -- only where those fall short.
- **Where the surface turns**, a 241-point tile's grid has SHAP7 under
  little of it, and its solution goes astray: those places are planned
  again with 121 points a side. Where even that fails -- 138 places -- no
  tile, and SHAP7 stays.
- **Each tile solved** from SHAP7 under it, on two levels, what the rest of
  the comet hides of it or shadows masked (`Surface.mask`), the images
  finer than 0.4 of the spacing compared triangle by triangle. **First**
  the 241-point tiles, each image's pointing searched 24 spacings on the
  ground: an image locks on its own, wrongly in a quarter of tile-image
  pairs. **Then** `kalast.shape.pointing` fits each image's pointing and
  each tile's offset to every registration at once -- an image is in
  dozens of tiles -- as SPC and MPCD fix an image's pointing outside the
  shape fit; and **every tile again**, each image at its pointing, a few
  pixels searched. On the held-out days: 0.58 against the first pass's
  0.50 and SHAP7's 0.34. The 121-point tiles are solved in the second
  pass alone: too little ground to register an image on its own.
- **Into one mesh**: `kalast.shape.compose` puts tiles into SHAP7 -- the
  tiles an image sees, at the image's scale -- and
  `kalast.mesh.Mesh.from_arrays` makes it a body.

`notes/2026-10-15_67p_tiles/` and `notes/2026-10-16_67p_second_pass/` have
how the tiles were laid, solved and checked.
